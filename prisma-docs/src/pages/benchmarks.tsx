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
  group: 'prisma' | 'xray' | 'singbox' | 'baseline';
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
  download_small_mbps: number;
  download_medium_mbps: number;
  download_large_mbps: number;
  upload_small_mbps: number;
  upload_medium_mbps: number;
  upload_large_mbps: number;
  cpu_sd_pct: number;
  memory_load_sd_kb: number;
  download_stable: boolean;
  upload_stable: boolean;
  concurrent_stable: boolean;
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
    case 'singbox':  return styles.barFillSingbox;
    default:         return styles.barFillBaseline;
  }
}

function groupLabelClass(group: string): string {
  switch (group) {
    case 'prisma':   return styles.groupLabelPrisma;
    case 'xray':     return styles.groupLabelXray;
    case 'singbox':  return styles.groupLabelSingbox;
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
        <span className={`${styles.legendDot} ${styles.legendSingbox}`} />
        sing-box
      </span>
      <span className={styles.legendItem}>
        <span className={`${styles.legendDot} ${styles.legendBaseline}`} />
        {translate({id: 'benchmarks.legend.baseline', message: 'Baseline'})}
      </span>
    </div>
  );
}

function StabilityBadge({stable}: {stable: boolean}): ReactNode {
  if (stable) return null;
  return (
    <span className={styles.unstableBadge} title="High variance detected in measurement">
      {'\u26A0'}
    </span>
  );
}

function DirectionHint({higher}: {higher: boolean}): ReactNode {
  return (
    <span className={styles.directionHint}>
      {higher ? '\u2191 higher is better' : '\u2193 lower is better'}
    </span>
  );
}

function hasPerSizeData(scenarios: Scenario[]): boolean {
  return scenarios.some(s => s.download_small_mbps > 0 || s.download_medium_mbps > 0 || s.download_large_mbps > 0);
}

function ThroughputChart({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const maxDownload = Math.max(...scenarios.map(s => s.download_mbps));
  const maxUpload = Math.max(...scenarios.map(s => s.upload_mbps));
  const maxVal = Math.max(maxDownload, maxUpload);
  const showPerSize = hasPerSizeData(scenarios);
  const baseline = scenarios.find(s => s.group === 'baseline');

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.throughput.title', message: 'Throughput Comparison'})}
      </Heading>
      <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', marginBottom: '1rem'}}>
        {translate({id: 'benchmarks.throughput.desc', message: 'Single-stream transfer speed through the proxy tunnel.'})}
      </p>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.throughput.download', message: 'Download (Mbps)'})}
          {showPerSize && <span style={{fontSize: '0.7rem', marginLeft: '0.5rem', opacity: 0.7}}>weighted: 0.3S + 0.4M + 0.3L</span>}
          <DirectionHint higher={true} />
        </div>
        {scenarios.map(s => (
          <div className={styles.barRow} key={`dl-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>
              {s.name}
              <StabilityBadge stable={s.download_stable ?? true} />
            </span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.download_mbps / maxVal) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.download_mbps, 0)} Mbps</span>
            </div>
            <span className={styles.barValue}>
              {fmt(s.download_mbps, 0)}
              {baseline && s.group !== 'baseline' && baseline.download_mbps > 0 && (
                <span className={styles.baselinePct}>{Math.round(s.download_mbps / baseline.download_mbps * 100)}%</span>
              )}
            </span>
          </div>
        ))}
      </div>

      {showPerSize && (
        <div style={{overflowX: 'auto', marginBottom: '1.5rem'}}>
          <table className={styles.dataTable}>
            <thead>
              <tr>
                <th>{translate({id: 'benchmarks.throughput.scenario', message: 'Scenario'})}</th>
                <th>1MB</th>
                <th>32MB</th>
                <th>256MB</th>
                <th>{translate({id: 'benchmarks.throughput.composite', message: 'Composite'})}</th>
              </tr>
            </thead>
            <tbody>
              {scenarios.filter(s => s.download_small_mbps > 0 || s.download_medium_mbps > 0 || s.download_large_mbps > 0).map(s => (
                <tr key={`dl-detail-${s.label}`}>
                  <td className={groupLabelClass(s.group)}>{s.name}</td>
                  <td>{fmt(s.download_small_mbps, 0)}</td>
                  <td>{fmt(s.download_medium_mbps, 0)}</td>
                  <td>{fmt(s.download_large_mbps, 0)}</td>
                  <td><strong>{fmt(s.download_mbps, 0)}</strong></td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.throughput.upload', message: 'Upload (Mbps)'})}
          <DirectionHint higher={true} />
        </div>
        {scenarios.map(s => (
          <div className={styles.barRow} key={`ul-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>
              {s.name}
              <StabilityBadge stable={s.upload_stable ?? true} />
            </span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.upload_mbps / maxVal) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.upload_mbps, 0)} Mbps</span>
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
      <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', marginBottom: '1rem'}}>
        {translate({id: 'benchmarks.latency.ttfb.desc', message: 'Time from request to first response byte through the tunnel.'})}
      </p>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.latency.ttfb', message: 'TTFB Latency (ms)'})}
          <DirectionHint higher={false} />
        </div>
        {proxied.map(s => (
          <div className={styles.barRow} key={`lat-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.latency_ms / maxLatency) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.latency_ms, 2)} ms</span>
            </div>
            <span className={styles.barValue}>{fmt(s.latency_ms, 2)} ms</span>
          </div>
        ))}
      </div>

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.latency.handshake', message: 'Handshake Time (ms)'})}
          <DirectionHint higher={false} />
        </div>
        <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.85rem', marginBottom: '0.75rem', marginTop: '-0.25rem'}}>
          {translate({id: 'benchmarks.latency.handshake.desc', message: 'Connection setup time including TLS/QUIC negotiation.'})}
        </p>
        {proxied.map(s => (
          <div className={styles.barRow} key={`hs-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.handshake_ms / maxHandshake) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.handshake_ms, 2)} ms</span>
            </div>
            <span className={styles.barValue}>{fmt(s.handshake_ms, 2)} ms</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function FilterTabs({activeFilter, onFilterChange}: {activeFilter: string; onFilterChange: (f: string) => void}): ReactNode {
  const tabs = [
    {key: 'all', label: 'All'},
    {key: 'prisma', label: 'Prisma'},
    {key: 'singbox', label: 'sing-box'},
    {key: 'xray', label: 'Xray-core'},
  ];
  return (
    <div className={styles.filterTabs}>
      {tabs.map(tab => (
        <button
          key={tab.key}
          type="button"
          className={`${styles.filterTab} ${activeFilter === tab.key ? styles.filterTabActive : ''} ${tab.key !== 'all' ? styles[`filterTab_${tab.key}`] : ''}`}
          onClick={() => onFilterChange(tab.key)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

function PlatformSummary({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const groups = ['prisma', 'singbox', 'xray'] as const;
  const groupNames: Record<string, string> = {prisma: 'Prisma', singbox: 'sing-box', xray: 'Xray-core'};
  const cardClasses: Record<string, string> = {
    prisma: styles.platformCardPrisma,
    singbox: styles.platformCardSingbox,
    xray: styles.platformCardXray,
  };

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.platforms.title', message: 'Platform Overview'})}
      </Heading>
      <div className={styles.platformCards}>
        {groups.map(group => {
          const groupScenarios = scenarios.filter(s => s.group === group);
          if (groupScenarios.length === 0) return null;

          const bestDl = groupScenarios.reduce((a, b) => a.download_mbps > b.download_mbps ? a : b);
          const avgLatency = groupScenarios.reduce((sum, s) => sum + s.latency_ms, 0) / groupScenarios.length;
          const minMemory = Math.min(...groupScenarios.map(s => s.memory_idle_kb));
          const maxSecurity = Math.max(...groupScenarios.map(s => s.security_score));
          const bestSecTier = groupScenarios.reduce((a, b) => a.security_score > b.security_score ? a : b).security_tier;

          return (
            <div className={`${styles.platformCard} ${cardClasses[group]}`} key={group}>
              <div className={styles.platformCardTitle}>{groupNames[group]}</div>
              <div className={styles.platformCardBest}>Best: {bestDl.name}</div>
              <div className={styles.platformStat}>{fmt(bestDl.download_mbps, 0)} Mbps download</div>
              <div className={styles.platformStat}>{fmt(avgLatency, 2)} ms avg latency</div>
              <div className={styles.platformStat}>{minMemory.toLocaleString()} KB min memory</div>
              <div className={styles.platformStat}>Security: {bestSecTier} ({maxSecurity})</div>
              <div className={styles.platformCardCount}>{groupScenarios.length} scenarios</div>
            </div>
          );
        })}
      </div>
    </section>
  );
}

function HeadToHead({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const groups = ['prisma', 'singbox', 'xray'] as const;
  const baseline = scenarios.find(s => s.group === 'baseline');

  const bestByGroup = groups.map(group => {
    const groupScenarios = scenarios.filter(s => s.group === group);
    if (groupScenarios.length === 0) return null;
    return groupScenarios.reduce((a, b) => a.download_mbps > b.download_mbps ? a : b);
  }).filter((s): s is Scenario => s !== null);

  if (bestByGroup.length < 2) return null;

  const metrics: {key: string; label: string; getValue: (s: Scenario) => number; format: (v: number) => string; unit: string; lowerBetter?: boolean}[] = [
    {key: 'download', label: 'Download', getValue: s => s.download_mbps, format: v => fmt(v, 0), unit: 'Mbps'},
    {key: 'upload', label: 'Upload', getValue: s => s.upload_mbps, format: v => fmt(v, 0), unit: 'Mbps'},
    {key: 'latency', label: 'Latency', getValue: s => s.latency_ms, format: v => fmt(v, 2), unit: 'ms', lowerBetter: true},
    {key: 'memory', label: 'Memory', getValue: s => s.memory_idle_kb, format: v => v.toLocaleString(), unit: 'KB', lowerBetter: true},
    {key: 'security', label: 'Security', getValue: s => s.security_score, format: v => `${v}`, unit: ''},
  ];

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.h2h.title', message: 'Head-to-Head Comparison'})}
      </Heading>
      <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', marginBottom: '1rem'}}>
        {translate({id: 'benchmarks.h2h.description', message: 'Best scenario from each platform compared side-by-side.'})}
      </p>
      <div style={{overflowX: 'auto'}}>
        <table className={`${styles.dataTable} ${styles.h2hTable}`}>
          <thead>
            <tr>
              <th>{translate({id: 'benchmarks.h2h.metric', message: 'Metric'})}</th>
              {bestByGroup.map(s => (
                <th key={s.label} className={groupLabelClass(s.group)}>{s.name}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {metrics.map(m => {
              const values = bestByGroup.map(s => m.getValue(s));
              const winnerIdx = m.lowerBetter
                ? values.indexOf(Math.min(...values.filter(v => v > 0)))
                : values.indexOf(Math.max(...values));
              return (
                <tr key={m.key}>
                  <td>{m.label}</td>
                  {bestByGroup.map((s, i) => (
                    <td key={s.label} className={i === winnerIdx ? styles.h2hWinner : ''}>
                      {m.format(m.getValue(s))} {m.unit}
                    </td>
                  ))}
                </tr>
              );
            })}
            {baseline && baseline.download_mbps > 0 && (
              <tr>
                <td>vs Baseline</td>
                {bestByGroup.map(s => (
                  <td key={s.label}>
                    {Math.round((s.download_mbps / baseline.download_mbps) * 100)}%
                  </td>
                ))}
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function ResourceSection({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const proxied = scenarios.filter(s => s.group !== 'baseline');
  const sortedByCpu = [...proxied].sort((a, b) => a.cpu_avg_pct - b.cpu_avg_pct);
  const sortedByMem = [...proxied].sort((a, b) => a.memory_idle_kb - b.memory_idle_kb);
  const maxCpu = Math.max(...proxied.map(s => s.cpu_avg_pct));
  const maxMem = Math.max(...proxied.map(s => s.memory_idle_kb));

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.resources.title', message: 'Resource Usage'})}
      </Heading>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.resources.cpuChart', message: 'CPU Usage (%) — lower is better'})}
          <DirectionHint higher={false} />
        </div>
        {sortedByCpu.map(s => (
          <div className={styles.barRow} key={`cpu-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${maxCpu > 0 ? (s.cpu_avg_pct / maxCpu) * 100 : 0}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.cpu_avg_pct)}%</span>
            </div>
            <span className={styles.barValue}>{fmt(s.cpu_avg_pct)}%</span>
          </div>
        ))}
      </div>

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.resources.memChart', message: 'Memory Idle (KB) — lower is better'})}
          <DirectionHint higher={false} />
        </div>
        {sortedByMem.map(s => (
          <div className={styles.barRow} key={`mem-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${maxMem > 0 ? (s.memory_idle_kb / maxMem) * 100 : 0}%`}}
              />
              <span className={styles.barTooltip}>{s.memory_idle_kb.toLocaleString()} KB</span>
            </div>
            <span className={styles.barValue}>{s.memory_idle_kb.toLocaleString()}</span>
          </div>
        ))}
      </div>

      <div style={{overflowX: 'auto'}}>
        <table className={styles.dataTable}>
          <thead>
            <tr>
              <th>{translate({id: 'benchmarks.resources.scenario', message: 'Scenario'})}</th>
              <th>{translate({id: 'benchmarks.resources.cpu', message: 'CPU %'})}</th>
              <th>{translate({id: 'benchmarks.resources.cpuSd', message: 'CPU SD'})}</th>
              <th>{translate({id: 'benchmarks.resources.memIdle', message: 'Mem Idle (KB)'})}</th>
              <th>{translate({id: 'benchmarks.resources.memLoad', message: 'Mem Load (KB)'})}</th>
              <th>{translate({id: 'benchmarks.resources.memSd', message: 'Mem SD (KB)'})}</th>
              <th>{translate({id: 'benchmarks.resources.concurrent', message: '4x Concurrent (Mbps)'})}</th>
              <th>{translate({id: 'benchmarks.resources.dlCv', message: 'DL CV%'})}</th>
              <th>{translate({id: 'benchmarks.resources.stability', message: 'Stable'})}</th>
            </tr>
          </thead>
          <tbody>
            {scenarios.map(s => {
              const allStable = (s.download_stable ?? true) && (s.upload_stable ?? true) && (s.concurrent_stable ?? true);
              return (
                <tr key={s.label}>
                  <td className={groupLabelClass(s.group)}>{s.name}</td>
                  <td>{fmt(s.cpu_avg_pct)}</td>
                  <td>{fmt(s.cpu_sd_pct ?? 0)}</td>
                  <td>{s.memory_idle_kb.toLocaleString()}</td>
                  <td>{s.memory_load_kb.toLocaleString()}</td>
                  <td>{(s.memory_load_sd_kb ?? 0).toLocaleString()}</td>
                  <td>{fmt(s.concurrent_mbps, 0)}</td>
                  <td>{fmt(s.download_cv_pct)}</td>
                  <td>{allStable ? '\u2705' : <span className={styles.unstableBadge}>{'\u26A0'}</span>}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function EfficiencyChart({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const proxied = scenarios.filter(s => s.group !== 'baseline' && s.memory_load_kb > 0);

  const withEfficiency = proxied.map(s => ({
    ...s,
    mbpsPerMb: s.download_mbps / (s.memory_load_kb / 1024),
    mbpsPerCpu: s.cpu_avg_pct > 0 ? s.concurrent_mbps / s.cpu_avg_pct : 0,
  }));

  const sortedByMbpsMb = [...withEfficiency].sort((a, b) => b.mbpsPerMb - a.mbpsPerMb);
  const sortedByMbpsCpu = [...withEfficiency].filter(s => s.mbpsPerCpu > 0).sort((a, b) => b.mbpsPerCpu - a.mbpsPerCpu);
  const maxMbpsMb = Math.max(...sortedByMbpsMb.map(s => s.mbpsPerMb));
  const maxMbpsCpu = sortedByMbpsCpu.length > 0 ? Math.max(...sortedByMbpsCpu.map(s => s.mbpsPerCpu)) : 1;

  return (
    <section className={styles.section}>
      <Heading as="h2" className={styles.sectionTitle}>
        {translate({id: 'benchmarks.efficiency.title', message: 'Throughput Efficiency'})}
      </Heading>
      <p style={{color: 'var(--ifm-font-color-secondary)', fontSize: '0.9rem', marginBottom: '1rem'}}>
        {translate({id: 'benchmarks.efficiency.description', message: 'Which proxy delivers the most throughput per unit of resource consumed.'})}
      </p>
      <Legend />

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.efficiency.perMb', message: 'Mbps per MB RAM (higher is better)'})}
          <DirectionHint higher={true} />
        </div>
        {sortedByMbpsMb.map(s => (
          <div className={styles.barRow} key={`eff-mb-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.mbpsPerMb / maxMbpsMb) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.mbpsPerMb, 1)} Mbps/MB</span>
            </div>
            <span className={styles.barValue}>{fmt(s.mbpsPerMb, 1)}</span>
          </div>
        ))}
      </div>

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.efficiency.perCpu', message: 'Mbps per CPU% (higher is better)'})}
          <DirectionHint higher={true} />
        </div>
        {sortedByMbpsCpu.map(s => (
          <div className={styles.barRow} key={`eff-cpu-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${groupBarClass(s.group)}`}
                style={{width: `${(s.mbpsPerCpu / maxMbpsCpu) * 100}%`}}
              />
              <span className={styles.barTooltip}>{fmt(s.mbpsPerCpu, 1)} Mbps/%</span>
            </div>
            <span className={styles.barValue}>{fmt(s.mbpsPerCpu, 1)}</span>
          </div>
        ))}
      </div>
    </section>
  );
}

function SecuritySection({scenarios}: {scenarios: Scenario[]}): ReactNode {
  const proxied = scenarios.filter(s => s.group !== 'baseline');
  const sorted = [...proxied].sort((a, b) => b.security_score - a.security_score);
  const maxScore = Math.max(...sorted.map(s => s.security_score), 1);

  function scoreTierClass(tier: string): string {
    switch (tier) {
      case 'S': return styles.barFillSecurityS;
      case 'A': return styles.barFillSecurityA;
      case 'B': return styles.barFillSecurityB;
      default:  return styles.barFillSecurityC;
    }
  }

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

      <div className={styles.chartGroup}>
        <div className={styles.chartGroupTitle}>
          {translate({id: 'benchmarks.security.chart', message: 'Security Score (higher is better)'})}
          <DirectionHint higher={true} />
        </div>
        {sorted.map(s => (
          <div className={styles.barRow} key={`sec-${s.label}`}>
            <span className={styles.barLabel} title={s.name}>{s.name}</span>
            <div className={styles.barTrack}>
              <div
                className={`${styles.barFill} ${scoreTierClass(s.security_tier)}`}
                style={{width: `${(s.security_score / 100) * 100}%`}}
              />
              <span className={styles.barTooltip}>{s.security_score}/100 ({s.security_tier})</span>
            </div>
            <span className={styles.barValue}>
              <span className={`${styles.tierBadge} ${tierClass(s.security_tier)}`} style={{width: '22px', height: '22px', fontSize: '0.75rem', marginRight: '4px'}}>
                {s.security_tier}
              </span>
              {s.security_score}
            </span>
          </div>
        ))}
      </div>

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
                    winner.group === 'prisma' ? styles.cardWinnerPrisma
                    : winner.group === 'singbox' ? styles.cardWinnerSingbox
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
  const [activeFilter, setActiveFilter] = useState<string>('all');

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
          message: 'Performance benchmarks comparing Prisma Proxy against Xray-core and sing-box',
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
          message: 'Performance benchmarks comparing Prisma Proxy against Xray-core and sing-box',
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

  // Filter out scenarios that failed to benchmark (all-zero results)
  const testedScenarios = data.scenarios.filter(s =>
    s.group === 'baseline' || s.download_mbps > 0 || s.upload_mbps > 0 || s.latency_ms > 0,
  );

  const sortedScenarios = [...testedScenarios].sort((a, b) => {
    const order: Record<string, number> = {prisma: 0, singbox: 1, xray: 2, baseline: 3};
    return (order[a.group] ?? 9) - (order[b.group] ?? 9);
  });

  const filteredScenarios = activeFilter === 'all'
    ? sortedScenarios
    : sortedScenarios.filter(s => s.group === 'baseline' || s.group === activeFilter);

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
        message: 'Performance benchmarks comparing Prisma Proxy against Xray-core and sing-box',
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
              message: 'Prisma Proxy vs Xray-core vs sing-box — automated weekly comparison',
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
              {testedScenarios.length}{' '}
              {translate({id: 'benchmarks.meta.scenarios', message: 'scenarios'})}
            </span>
          </div>
        </div>

        {/* Filter tabs */}
        <FilterTabs activeFilter={activeFilter} onFilterChange={setActiveFilter} />

        {/* Platform summary */}
        <PlatformSummary scenarios={sortedScenarios} />

        {/* Charts and tables */}
        <ThroughputChart scenarios={filteredScenarios} />

        {/* Head-to-head comparison */}
        <HeadToHead scenarios={sortedScenarios} />

        <LatencyChart scenarios={filteredScenarios} />
        <ResourceSection scenarios={filteredScenarios} />

        {/* Throughput Efficiency */}
        <EfficiencyChart scenarios={filteredScenarios} />

        <SecuritySection scenarios={filteredScenarios} />
        <UseCaseCards scenarios={filteredScenarios} />
        <HistorySection />
      </main>
    </Layout>
  );
}
