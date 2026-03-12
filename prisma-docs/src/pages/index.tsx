import type {ReactNode} from 'react';
import clsx from 'clsx';
import Link from '@docusaurus/Link';
import Translate, {translate} from '@docusaurus/Translate';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import Layout from '@theme/Layout';
import Heading from '@theme/Heading';

import styles from './index.module.css';

const features = [
  {
    titleId: 'homepage.features.dualTransport.title',
    titleDefault: 'Dual Transport',
    descId: 'homepage.features.dualTransport.description',
    descDefault: 'QUIC primary with TCP fallback. Double encryption via PrismaVeil inside QUIC/TLS for defense-in-depth.',
  },
  {
    titleId: 'homepage.features.modernCrypto.title',
    titleDefault: 'Modern Cryptography',
    descId: 'homepage.features.modernCrypto.description',
    descDefault: 'X25519 ECDH, BLAKE3 KDF, ChaCha20-Poly1305 / AES-256-GCM AEAD with anti-replay protection.',
  },
  {
    titleId: 'homepage.features.portForwarding.title',
    titleDefault: 'Port Forwarding',
    descId: 'homepage.features.portForwarding.description',
    descDefault: 'Expose local services through the server with frp-style reverse proxy over encrypted tunnels.',
  },
];

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          {siteConfig.title}
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/introduction">
            <Translate id="homepage.getStarted">Get Started</Translate>
          </Link>
        </div>
      </div>
    </header>
  );
}

export default function Home(): ReactNode {
  return (
    <Layout
      title={translate({id: 'homepage.title', message: 'Home'})}
      description={translate({id: 'homepage.description', message: 'Prisma Proxy — next-generation encrypted proxy infrastructure built in Rust'})}>
      <HomepageHeader />
      <main>
        <div className="container" style={{padding: '2rem 0'}}>
          <div className="row">
            {features.map(({titleId, titleDefault, descId, descDefault}) => (
              <div key={titleId} className="col col--4">
                <Heading as="h3">
                  <Translate id={titleId}>{titleDefault}</Translate>
                </Heading>
                <p>
                  <Translate id={descId}>{descDefault}</Translate>
                </p>
              </div>
            ))}
          </div>
        </div>
      </main>
    </Layout>
  );
}
