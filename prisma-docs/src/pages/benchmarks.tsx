import type {ReactNode} from 'react';
import {useState, useEffect} from 'react';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';
import Link from '@docusaurus/Link';
import {translate} from '@docusaurus/Translate';
import useBaseUrl from '@docusaurus/useBaseUrl';

import styles from './benchmarks.module.css';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface Scenario {
  label: string;
  name: string;
  group: 'prisma' | 'xray' | 'baseline';
  download_mbps: number;
  upload_mbps: number;
  latency_ms: number;
  handshake_ms: number;
  concurrent_mbps: number;
  memory_idle_kb: number;
  memory_load_kb: number;
  cpu_avg_pct: number;
  download_cv_pct: number;
  concurrent_cv_pct: number;
  security_score: number;
  security_tier: string;
}

interface BenchmarkData {
  timestamp: string;
  environment: string;
  scenarios: Scenario[];
  history: unknown[];
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const REPO_URL = 'https://github.com/Yamimega/prisma';
const ACTIONS_URL = `${REPO_URL}/actions/workflows/benchmark.yml`;

interface UseCaseProfile {
  id: string;
  icon: string;
  title: string;
  weights: Record<string, number>;
  description: string;
}

function getUseCaseProfiles(): UseCaseProfile[] {
  return [
    {
      id: 'personal',
      icon: '\uD83D\uDD12',
      title: translate({id: 'benchmarks.usecase.personal', message: 'Personal VPN'}),
      weights: {download_mbps: 15, upload_mbps: 5, latency_ms: 20, handshake_ms: 5, concurrent_mbps: 10, cpu_avg_pct: 5, memory_idle_kb: 5, security_score: 25},
      description: translate({id: 'benchmarks.usecase.personal.desc', message: 'Daily browsing, streaming, and privacy protection. Prioritizes low latency and strong security.'}),
    },
    {
      id: 'saas',
      icon: '\uD83C\uDFE2',
      title: translate({id: 'benchmarks.usecase.saas', message: 'Multi-Tenant SaaS'}),
      weights: {download_mbps: 10, upload_mbps: 10, latency_ms: 10, handshake_ms: 5, concurrent_mbps: 20, cpu_avg_pct: 10, memory_idle_kb: 10, security_score: 15},
      description: translate({id: 'benchmarks.usecase.saas.desc', message: 'Serving many concurrent users. Prioritizes concurrency and balanced resource usage.'}),
    },
    {
      id: 'edge',
      icon: '\uD83D\uDCE1',
      title: translate({id: 'benchmarks.usecase.edge', message: 'Edge / IoT'}),
      weights: {download_mbps: 10, upload_mbps: 5, latency_ms: 5, handshake_ms: 5, concurrent_mbps: 10, cpu_avg_pct: 15, memory_idle_kb: 20, security_score: 15},
      description: translate({id: 'benchmarks.usecase.edge.desc', message: 'Resource-constrained devices. Prioritizes low memory footprint and CPU efficiency.'}),
    },
    {
      id: 'cdn',
      icon: '\uD83D\uDCE6',
      title: translate({id: 'benchmarks.usecase.cdn', message: 'CDN / Bulk Transfer'}),
      weights: {download_mbps: 25, upload_mbps: 10, latency_ms: 5, handshake_ms: 5, concurrent_mbps: 20, cpu_avg_pct: 10, memory_idle_kb: 5, security_score: 10},
      description: translate({id: 'benchmarks.usecase.cdn.desc', message: 'Large file transfers and content delivery. Prioritizes raw throughput and concurrency.'}),
    },
    {
      id: 'highsec',
      icon: '\uD83D\uDEE1\uFE0F',
      title: translate({id: 'benchmarks.usecase.highsec', message: 'High-Security'}),
      weights: {download_mbps: 5, upload_mbps: 5, latency_ms: 5, handshake_ms: 5, concurrent_mbps: 5, cpu_avg_pct: 5, memory_idle_kb: 5, security_score: 60},
      description: translate({id: 'benchmarks.usecase.highsec.desc', message: 'Maximum censorship resistance. Security score is the dominant factor.'}),
    },
  ];
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function fmt(v: number, decimals = 1): string {
  if (v >= 1000) {
    return v.toLocaleString(undefined, {maximumFractionDigits: 0});
  }
  return v.toFixed(decimals);
}

function tierClass(tier: string): string {
  switch (tier) {
    case 'S': return styles.tierS;
    case 'A': return styles.tierA;
    case 'B': return styles.tierB;
    case 'C': return styles.tierC;
    default:  return styles.tierNone;
  }
}

function groupBarClass(group: string): string {
  switch (group) {
    case 'prisma':   return styles.barFillPrisma;
    case 'xray':     return styles.barFillXray;
    default:         return styles.barFillBaseline;
  }
}

function groupLabelClass(group: string): string {
  switch (group) {
    case 'prisma':   return styles.groupLabelPrisma;
    case 'xray':     return styles.groupLabelXray;
    default:         return styles.groupLabelBaseline;
  }
}

function scoreUseCase(scenario: Scenario, weights: Record<string, number>, allScenarios: Scenario[]): number {
  const maxVals: Record<string, number> = {};
  const minVals: Record<string, number> = {};
  const fields = Object.keys(weights);
  for (const f of fields) {
    const vals = allScenarios
      .filter(s => s.group !== 'baseline')
      .map(s => (s as unknown as Record<string, number>)[f] ?? 0);
    maxVals[f] = Math.max(...vals);
    minVals[f] = Math.min(...vals);
  }

  const higherBetter = new Set(['download_mbps', 'upload_mbps', 'concurrent_mbps', 'security_score']);
  let total = 0;
  let weightSum = 0;
  for (const f of fields) {
    const w = weights[f];
    const v = (scenario as unknown as Record<string, number>)[f] ?? 0;
    const range = maxVals[f] - minVals[f];
    let norm: number;
    if (range === 0) {
      norm = 1;
    } else if (higherBetter.has(f)) {
      norm = (v - minVals[f]) / range;
    } else {
      norm = (maxVals[f] - v) / range;
    }
    total += norm * w;
    weightSum += w;
  }
  return weightSum > 0 ? (total / weightSum) * 100 : 0;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function Legend(): ReactNode {
  return (
    <div className={styles.legend}>
      <span className={styles.legendItem}>
        <span className={`${styles.legendDot} ${styles.legendPrisma}`} />
        Prisma
      </span>
      <span className={styles.legendItem}>
        <span className={`${styles.legendDot} ${styles.legendXray}`} />
        Xray-core
      </span>
      <span className={styles.legendItem}>
        <span className={`${styles.legendDot} ${styles.legendBaseline}`} />
        {translate({id: 'benchmarks.legend.baseline', message: 'Baseline'})}
      </span>
    </div>
  );
}

function ThroughputChart({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const maxDownload = Math.max(...scenarios.map(s => s.download_mbps));
  const maxUpload = Math.max(...scenarios.map(s => s.upload_mbps));
  const maxVal = Math.max(maxDownload, maxUpload);

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.throughput.title', message: 'Throughput Comparison'})}
      </Heading>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.throughput.download', message: 'Download (Mbps)'})}
        </div>
        {scenarios.map(s => (
          <div className={styles.barRow} key={`dl-${s.label}`}>
            <span className={styles.barLabel}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.download_mbps / maxVal) * 100}%`}}
              />
            </div>
            <span className={styles.barValue}>{fmt(s.download_mbps, 0)}</span>
          </div>
        ))}
      </div>

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.throughput.upload', message: 'Upload (Mbps)'})}
        </div>
        {scenarios.map(s => (
          <div className={styles.barRow} key={`ul-${s.label}`}>
            <span className={styles.barLabel}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.upload_mbps / maxVal) * 100}%`}}
              />
            </div>
            <span className={styles.barValue}>{fmt(s.upload_mbps, 0)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function LatencyChart({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const proxied = scenarios.filter(s => s.group !== 'baseline');
  const maxLatency = Math.max(...proxied.map(s => s.latency_ms));
  const maxHandshake = Math.max(...proxied.map(s => s.handshake_ms));

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.latency.title', message: 'Latency Comparison'})}
      </Heading>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.latency.ttfb', message: 'TTFB Latency (ms)'})}
        </div>
        {proxied.map(s => (
          <div className={styles.barRow} key={`lat-${s.label}`}>
            <span className={styles.barLabel}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.latency_ms / maxLatency) * 100}%`}}
              />
            </div>
            <span className={styles.barValue}>{fmt(s.latency_ms, 2)} ms</span>
          </div>
        ))}
      </div>

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.latency.handshake', message: 'Handshake Time (ms)'})}
        </div>
        {proxied.map(s => (
          <div className={styles.barRow} key={`hs-${s.label}`}>
            <span className={styles.barLabel}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.handshake_ms / maxHandshake) * 100}%`}}
              />
            </div>
            <span className={styles.barValue}>{fmt(s.handshake_ms, 2)} ms</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function ResourceTable({scenarios}: {scenarios: Scenario[]}): ReactNode {
  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.resources.title', message: 'Resource Usage'})}
      </Heading>
      <div style={{overflowX: 'auto'}}>
        <table className={styles.dataTable}>
          <thead>
            <tr>
              <th>{translate({id: 'benchmarks.resources.scenario', message: 'Scenario'})}</th>
              <th>{translate({id: 'benchmarks.resources.cpu', message: 'CPU %'})}</th>
              <th>{translate({id: 'benchmarks.resources.memIdle', message: 'Memory Idle (KB)'})}</th>
              <th>{translate({id: 'benchmarks.resources.memLoad', message: 'Memory Load (KB)'})}</th>
              <th>{translate({id: 'benchmarks.resources.concurrent', message: '4x Concurrent (Mbps)'})}</th>
              <th>{translate({id: 'benchmarks.resources.dlCv', message: 'DL Stability (CV%)'})}</th>
            </tr>
          </thead>
          <tbody>
            {scenarios.map(s => (
              <tr key={s.label}>
                <td className={groupLabelClass(s.group)}>{s.name}</td>
                <td>{fmt(s.cpu_avg_pct)}</td>
                <td>{s.memory_idle_kb.toLocaleString()}</td>
                <td>{s.memory_load_kb.toLocaleString()}</td>
                <td>{fmt(s.concurrent_mbps, 0)}</td>
                <td>{fmt(s.download_cv_pct)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function SecurityTable({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const proxied = scenarios.filter(s => s.group !== 'baseline');
  const sorted = [...proxied].sort((a, b) => b.security_score - a.security_score);

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.security.title', message: 'Security Scoring'})}
      </Heading>
      <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', marginBottom: '1rem'}}>
        {translate({
          id: 'benchmarks.security.description',
          message: 'Composite score (0-100) based on encryption depth, forward secrecy, traffic analysis resistance, protocol detection resistance, anti-replay, and auth strength.',
        })}
      </p>
      <div style={{overflowX: 'auto'}}>
        <table className={styles.dataTable}>
          <thead>
            <tr>
              <th>{translate({id: 'benchmarks.security.scenario', message: 'Scenario'})}</th>
              <th>{translate({id: 'benchmarks.security.tier', message: 'Tier'})}</th>
              <th>{translate({id: 'benchmarks.security.score', message: 'Score'})}</th>
              <th>{translate({id: 'benchmarks.security.throughput', message: 'Download (Mbps)'})}</th>
              <th>{translate({id: 'benchmarks.security.tradeoff', message: 'Security/Speed'})}</th>
            </tr>
          </thead>
          <tbody>
            {sorted.map(s => (
              <tr key={s.label}>
                <td className={groupLabelClass(s.group)}>{s.name}</td>
                <td>
                  <span className={`${styles.tierBadge} ${tierClass(s.security_tier)}`}>
                    {s.security_tier}
                  </span>
                </td>
                <td>{s.security_score}/100</td>
                <td>{fmt(s.download_mbps, 0)}</td>
                <td>{fmt(s.security_score * s.download_mbps / 100, 0)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function UseCaseCards({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const profiles = getUseCaseProfiles();
  const proxied = scenarios.filter(s => s.group !== 'baseline');

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.usecases.title', message: 'Use-Case Recommendations'})}
      </Heading>
      <div className={styles.cardGrid}>
        {profiles.map(profile => {
          const scored = proxied.map(s => ({
            scenario: s,
            score: scoreUseCase(s, profile.weights, scenarios),
          }));
          scored.sort((a, b) => b.score - a.score);
          const winner = scored[0]?.scenario;
          const winnerScore = scored[0]?.score ?? 0;

          return (
            <div className={styles.card} key={profile.id}>
              <div className={styles.cardTitle}>
                <span className={styles.cardIcon}>{profile.icon}</span>
                {profile.title}
              </div>
              {winner && (
                <div
                  className={`${styles.cardWinner} ${
                    winner.group === 'prisma'
                      ? styles.cardWinnerPrisma
                      : styles.cardWinnerXray
                  }`}
                >
                  {winner.name} ({fmt(winnerScore, 0)}%)
                </div>
              )}
              <p className={styles.cardReason}>{profile.description}</p>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function HistorySection(): ReactNode {
  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.history.title', message: 'Historical Trends'})}
      </Heading>
      <div className={styles.historyPlaceholder}>
        <p>
          {translate({
            id: 'benchmarks.history.placeholder',
            message: 'Historical benchmark data is collected on each CI run. View past results and trends directly on GitHub Actions.',
          })}
        </p>
        <Link className="button button--primary button--sm" href={ACTIONS_URL}>
          {translate({id: 'benchmarks.history.link', message: 'View Historical Data on GitHub Actions'})}
        </Link>
      </div>
    </section>
  );
}

// ---------------------------------------------------------------------------
// Main page component
// ---------------------------------------------------------------------------

export default function BenchmarksPage(): ReactNode {
  const dataUrl = useBaseUrl('/data/benchmark-results.json');
  const [data, setData] = useState<BenchmarkData | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  function loadData() {
    setLoading(true);
    setError(null);
    fetch(dataUrl)
      .then(res => {
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return res.json();
      })
      .then((json: BenchmarkData) => {
        setData(json);
        setLoading(false);
      })
      .catch(() => {
        // Fallback: try GitHub API for latest artifact metadata
        fetch(`https://api.github.com/repos/Yamimega/prisma/actions/artifacts?per_page=1&name=benchmark-results`)
          .then(res => res.json())
          .then(json => {
            if (json.artifacts && json.artifacts.length > 0) {
              setError(
                translate({
                  id: 'benchmarks.error.artifactOnly',
                  message: 'Benchmark data file not yet deployed. Latest CI artifact available on GitHub Actions.',
                }),
              );
            } else {
              setError(
                translate({
                  id: 'benchmarks.error.noData',
                  message: 'Unable to load benchmark data. Please check back later or view results on GitHub Actions.',
                }),
              );
            }
            setLoading(false);
          })
          .catch(() => {
            setError(
              translate({
                id: 'benchmarks.error.fetch',
                message: 'Failed to fetch benchmark data. Please try again later.',
              }),
            );
            setLoading(false);
          });
      });
  }

  useEffect(() => {
    loadData();
  }, [dataUrl]);

  if (loading) {
    return (
      <Layout
        title={translate({id: 'benchmarks.page.title', message: 'Benchmarks'})}
        description={translate({
          id: 'benchmarks.page.description',
          message: 'Performance benchmarks comparing Prisma Proxy against Xray-core',
        })}
      >
        <main className={styles.loading}>
          <div className={styles.spinner} />
          <p>{translate({id: 'benchmarks.loading', message: 'Loading benchmark data...'})}</p>
        </main>
      </Layout>
    );
  }

  if (error || !data) {
    return (
      <Layout
        title={translate({id: 'benchmarks.page.title', message: 'Benchmarks'})}
        description={translate({
          id: 'benchmarks.page.description',
          message: 'Performance benchmarks comparing Prisma Proxy against Xray-core',
        })}
      >
        <main className={`container ${styles.error}`}>
          <Heading as="h2">
            {translate({id: 'benchmarks.error.title', message: 'Could not load benchmarks'})}
          </Heading>
          <p>{error}</p>
          <button
            className={`button button--primary ${styles.retryBtn}`}
            onClick={loadData}
            type="button"
          >
            {translate({id: 'benchmarks.error.retry', message: 'Retry'})}
          </button>
          <div style={{marginTop: '1rem'}}>
            <Link href={ACTIONS_URL}>
              {translate({id: 'benchmarks.error.viewGH', message: 'View results on GitHub Actions'})}
            </Link>
          </div>
        </main>
      </Layout>
    );
  }

  const sortedScenarios = [...data.scenarios].sort((a, b) => {
    const order: Record<string, number> = {prisma: 0, xray: 1, baseline: 2};
    return (order[a.group] ?? 9) - (order[b.group] ?? 9);
  });

  const updatedDate = new Date(data.timestamp).toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    timeZoneName: 'short',
  });

  return (
    <Layout
      title={translate({id: 'benchmarks.page.title', message: 'Benchmarks'})}
      description={translate({
        id: 'benchmarks.page.description',
        message: 'Performance benchmarks comparing Prisma Proxy against Xray-core',
      })}
    >
      <main className={`container ${styles.page}`}>
        {/* Header */}
        <div className={styles.header}>
          <Heading as="h1" className={styles.title}>
            {translate({id: 'benchmarks.heading', message: 'Performance Benchmarks'})}
          </Heading>
          <p className={styles.subtitle}>
            {translate({
              id: 'benchmarks.subtitle',
              message: 'Prisma Proxy vs Xray-core — automated weekly comparison',
            })}
          </p>
          <div className={styles.meta}>
            <span className={styles.metaItem}>
              <span className={styles.metaIcon}>{'\uD83D\uDCC5'}</span>
              {translate({id: 'benchmarks.meta.updated', message: 'Last updated:'})}{' '}
              {updatedDate}
            </span>
            <span className={styles.metaItem}>
              <span className={styles.metaIcon}>{'\u2699\uFE0F'}</span>
              {data.environment}
            </span>
            <span className={styles.metaItem}>
              <span className={styles.metaIcon}>{'\uD83D\uDCCA'}</span>
              {data.scenarios.length}{' '}
              {translate({id: 'benchmarks.meta.scenarios', message: 'scenarios'})}
            </span>
          </div>
        </div>

        {/* Charts and tables */}
        <ThroughputChart scenarios={sortedScenarios} />
        <LatencyChart scenarios={sortedScenarios} />
        <ResourceTable scenarios={sortedScenarios} />
        <SecurityTable scenarios={sortedScenarios} />
        <UseCaseCards scenarios={sortedScenarios} />
        <HistorySection />
      </main>
    </Layout>
  );
}
