import type {ReactNode} from 'react';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

function getFeatures() {
  return [
    {
      title: translate({id: 'homepage.features.multiTransport.title', message: 'Multi-Transport'}),
      icon: '🔀',
      description: translate({id: 'homepage.features.multiTransport.description', message: 'QUIC v2, TCP+TLS, WebSocket, gRPC, XHTTP, XPorta — auto-fallback across transports when censors block one.'}),
    },
    {
      title: translate({id: 'homepage.features.prismaTls.title', message: 'PrismaTLS'}),
      icon: '🛡️',
      description: translate({id: 'homepage.features.prismaTls.description', message: 'Active probing resistance with browser fingerprint mimicry, mask server pool, and padding beacon authentication.'}),
    },
    {
      title: translate({id: 'homepage.features.trafficShaping.title', message: 'Traffic Shaping'}),
      icon: '📊',
      description: translate({id: 'homepage.features.trafficShaping.description', message: 'Bucket padding, chaff injection, timing jitter, and frame coalescing defeat encapsulated TLS fingerprinting.'}),
    },
    {
      title: translate({id: 'homepage.features.cdnCompatible.title', message: 'CDN Compatible'}),
      icon: '☁️',
      description: translate({id: 'homepage.features.cdnCompatible.description', message: 'Hide your server behind Cloudflare. XPorta makes traffic indistinguishable from normal REST API calls.'}),
    },
    {
      title: translate({id: 'homepage.features.tunMode.title', message: 'TUN Mode'}),
      icon: '🌐',
      description: translate({id: 'homepage.features.tunMode.description', message: 'System-wide proxy via virtual network interface. All apps proxied automatically — no per-app configuration.'}),
    },
    {
      title: translate({id: 'homepage.features.builtInRust.title', message: 'Built in Rust'}),
      icon: '⚡',
      description: translate({id: 'homepage.features.builtInRust.description', message: 'Zero-copy I/O, async runtime, memory safety. Handles thousands of concurrent connections with minimal resources.'}),
    },
  ];
}

function getHighlights() {
  return [
    { label: translate({id: 'homepage.highlights.encryption', message: 'Encryption'}), value: 'ChaCha20-Poly1305 / AES-256-GCM' },
    { label: translate({id: 'homepage.highlights.keyExchange', message: 'Key Exchange'}), value: 'X25519 ECDH + BLAKE3 KDF' },
    { label: translate({id: 'homepage.highlights.handshake', message: 'Handshake'}), value: '1 RTT (0-RTT with tickets)' },
    { label: translate({id: 'homepage.highlights.udpRelay', message: 'UDP Relay'}), value: 'PrismaUDP + FEC Reed-Solomon' },
    { label: translate({id: 'homepage.highlights.congestion', message: 'Congestion'}), value: 'BBR / Brutal / Adaptive' },
    { label: translate({id: 'homepage.highlights.obfuscation', message: 'Obfuscation'}), value: 'Salamander v2 (nonce-based)' },
  ];
}

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={styles.hero}>
      <div className="container">
        <Heading as="h1" className={styles.title}>
          {siteConfig.title}
        </Heading>
        <p className={styles.tagline}>{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link className="button button--primary button--lg" to="/docs/introduction">
            <Translate id="homepage.getStarted">Get Started</Translate>
          </Link>
          <Link
            className={`button button--outline button--lg ${styles.btnOutline}`}
            to="https://github.com/Yamimega/prisma">
            <Translate id="homepage.viewGitHub">GitHub</Translate>
          </Link>
        </div>
      </div>
    </header>
  );
}

function FeatureCard({title, icon, description}: {title: string; icon: string; description: string}) {
  return (
    <div className={styles.featureCard}>
      <div className={styles.featureIcon}>{icon}</div>
      <Heading as="h3" className={styles.featureTitle}>{title}</Heading>
      <p className={styles.featureDescription}>{description}</p>
    </div>
  );
}

function FeaturesSection() {
  return (
    <section className={styles.features}>
      <div className="container">
        <Heading as="h2" className={styles.sectionTitle}>
          <Translate id="homepage.features">Features</Translate>
        </Heading>
        <div className={styles.featureGrid}>
          {getFeatures().map((f, i) => (
            <FeatureCard key={i} {...f} />
          ))}
        </div>
      </div>
    </section>
  );
}

function HighlightsSection() {
  return (
    <section className={styles.highlights}>
      <div className="container">
        <Heading as="h2" className={styles.sectionTitle}>
          <Translate id="homepage.protocol">Protocol v4</Translate>
        </Heading>
        <div className={styles.highlightGrid}>
          {getHighlights().map((h, i) => (
            <div key={i} className={styles.highlightItem}>
              <span className={styles.highlightLabel}>{h.label}</span>
              <span className={styles.highlightValue}>{h.value}</span>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function QuickStartSection() {
  return (
    <section className={styles.quickStart}>
      <div className="container">
        <Heading as="h2" className={styles.sectionTitle}>
          <Translate id="homepage.quickStart">Quick Start</Translate>
        </Heading>
        <div className={styles.codeBlock}>
          <code>curl -fsSL https://raw.githubusercontent.com/Yamimega/prisma/master/scripts/install.sh | bash -s -- --setup</code>
        </div>
        <p className={styles.quickStartNote}>
          <Translate id="homepage.quickStartNote">
            Installs the binary, generates credentials, TLS certificates, and example config files.
          </Translate>
        </p>
      </div>
    </section>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title={translate({id: 'homepage.title', message: 'Home'})}
      description={translate({
        id: 'homepage.description',
        message: 'Prisma Proxy — next-generation encrypted proxy infrastructure built in Rust',
      })}>
      <HomepageHeader />
      <main>
        <FeaturesSection />
        <HighlightsSection />
        <QuickStartSection />
      </main>
    </Layout>
  );
}
