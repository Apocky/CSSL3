// apocky.com/docs/mycelium

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="mycelium"
      title="Mycelium + Home · Apocky Docs"
      description="The Home pocket-dimension and the mycelial multiplayer thesis. Seven archetypes, five privacy modes, and what cross-instance learning does and does not do."
    >
      <h1 className="docs-h1">Mycelium + Home</h1>
      <p className="docs-blurb">§ Pocket-dimensions · 7 archetypes · 5 modes · what cross-instance learning does (and doesn't).</p>

      <h2 className="docs-h2">§ The thesis in one paragraph</h2>
      <p className="docs-p">
        Most multiplayer architectures fall on one of two poles: a centralized server that owns the canonical
        world-state, or a public-ledger blockchain that pays gas to write state. The mycelial-network thesis is
        that the actual organic shape of how multiplayer should feel is neither. Each player runs a sovereign
        Home (a pocket-dimension on their own machine), Home-to-Home traffic flows through Σ-mask-gated
        membranes, and KAN learning lets the topology adapt without a central coordinator.
      </p>

      <p className="docs-p">
        The long-form essay is at{' '}
        <a href="/devblog/the-mycelial-network-vision" style={{ color: '#7dd3fc', textDecoration: 'underline' }}>
          /devblog/the-mycelial-network-vision
        </a>. This page is the user-facing summary.
      </p>

      <h2 className="docs-h2">§ Home · the pocket-dimension</h2>
      <p className="docs-p">
        Your <strong>Home</strong> is a private space inside the LoA world that lives on your machine. It is
        always there when you launch; it never appears in any other player's instance unless you invite them.
        Home is where you craft, store, and call your own.
      </p>

      <h3 className="docs-h3">§ Seven archetypes</h3>
      <p className="docs-p">
        At first launch the substrate samples a Home archetype based on your seed. You can switch between them
        from the menu without losing inventory.
      </p>
      <table className="docs-table">
        <thead>
          <tr><th>Archetype</th><th>Inspiration</th><th>Vibe</th></tr>
        </thead>
        <tbody>
          <tr><td>Orbital</td><td>Destiny-2 Tower</td><td>Drifting platform · panoramic skybox</td></tr>
          <tr><td>Liset</td><td>Warframe Liset ship</td><td>Cozy ship interior · holographic UI</td></tr>
          <tr><td>Cathedral</td><td>Romanesque sanctuary</td><td>Tall stone vaults · candlelit</td></tr>
          <tr><td>Observatory</td><td>Astronomy + alchemy</td><td>Telescope · star-charts · brass tools</td></tr>
          <tr><td>Forest</td><td>Druidic grove</td><td>Living glade · seasons cycle</td></tr>
          <tr><td>Hybrid</td><td>Mix-and-match</td><td>Per-room substitutions</td></tr>
          <tr><td>Apocky</td><td>Author-mode</td><td>The unbounded substrate canvas · for power users</td></tr>
        </tbody>
      </table>

      <h2 className="docs-h2">§ Five privacy modes</h2>
      <p className="docs-p">
        Mycelium activity is opt-in per-mode. Mode-C is the default. You can change modes at any time from the
        Home menu; changes take effect on the next outbound mycelium frame.
      </p>
      <table className="docs-table">
        <thead>
          <tr><th>Mode</th><th>What is shared</th><th>Cap required</th></tr>
        </thead>
        <tbody>
          <tr><td>A · Private</td><td>Nothing · airgapped Home · single-player only</td><td>(none)</td></tr>
          <tr><td>B · Whitelist</td><td>Only with friends you have explicitly invited</td><td><code className="docs-ic">cap.mycelium=invitee</code></td></tr>
          <tr><td>C · Mycelium-Default</td><td>Federated bias-learning + drop-in invites · no canonical state shared</td><td><code className="docs-ic">cap.mycelium=discoverable</code></td></tr>
          <tr><td>D · Open Mesh</td><td>Public Home · others can drop-in · still Σ-mask-gated</td><td><code className="docs-ic">cap.mycelium=open</code></td></tr>
          <tr><td>E · Akashic Contributor</td><td>Long-term federated memory contribution · attested-anonymous</td><td><code className="docs-ic">cap.akashic=writer</code></td></tr>
        </tbody>
      </table>

      <Callout kind="success" title="Mode-A is real airgap">
        Mode-A doesn't just stop voluntary uploads — it disables the network thread entirely. The engine
        verifies this at boot and refuses to bring up the mycelium-runtime if Mode-A is set. You can run LoA
        on a machine that has no network adapter at all.
      </Callout>

      <h2 className="docs-h2">§ What cross-instance learning does</h2>
      <p className="docs-p">
        In Mode-C and above, the engine participates in <strong>federated bias-learning</strong>: small KAN
        edge updates flow between Homes. The shared object is <em>statistical bias</em>, not personal data.
        Concretely:
      </p>
      <ul className="docs-ul">
        <li>An NPC routine that players globally find more engaging will gradually become more common in NeverhomeRise.</li>
        <li>A craft recipe that survives more attestations will surface earlier in the recipe browser.</li>
        <li>A balance edge that other players' KANs converged on can ship as a hotfix without recompiling.</li>
      </ul>

      <h2 className="docs-h2">§ What cross-instance learning does NOT do</h2>
      <ul className="docs-ul">
        <li>It does not share your inventory, character build, or location.</li>
        <li>It does not share chat-panel text.</li>
        <li>It does not share screenshots, captures, or local logs.</li>
        <li>It does not share IP addresses with peers · routing is via the bootstrap rendezvous you choose.</li>
        <li>It does not share <em>anything</em> in Mode-A.</li>
      </ul>

      <Callout kind="warn" title="Pre-shipping notice">
        ◐ Mycelium multiplayer is wired in the substrate (<code className="docs-ic">cssl-host-mycelium</code>,
        <code className="docs-ic"> cssl-host-multiplayer</code>) but the user-facing UI for inviting friends, browsing
        public Homes, and reviewing federated KAN updates is still in build. The single-player Home experience
        is shippable today; the multiplayer surface is the next major release slice.
      </Callout>

      <h2 className="docs-h2">§ Where to read more</h2>
      <ul className="docs-ul">
        <li><a href="/devblog/the-mycelial-network-vision" style={{ color: '#7dd3fc' }}>devblog · long-form thesis</a></li>
        <li><a href="/docs/substrate" style={{ color: '#7dd3fc' }}>HDC · the messaging primitive mycelium uses</a></li>
        <li><a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>How cap.mycelium gates the network thread</a></li>
        <li>Spec · <code className="docs-ic">specs/grand-vision/16_MYCELIAL_NETWORK.csl</code></li>
        <li>Spec · <code className="docs-ic">specs/grand-vision/14_SIGMA_CHAIN.csl</code> · cross-Home consensus</li>
      </ul>

      <PrevNextNav slug="mycelium" />
    </DocsLayout>
  );
};

export default Page;
