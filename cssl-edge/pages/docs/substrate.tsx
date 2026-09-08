// apocky.com/docs/substrate

import type { NextPage } from 'next';
import DocsLayout from '@/components/DocsLayout';
import Callout from '@/components/Callout';
import CodeBlock from '@/components/CodeBlock';
import PrevNextNav from '@/components/PrevNextNav';

const Page: NextPage = () => {
  return (
    <DocsLayout
      activeSlug="substrate"
      title="Technical foundations · Apocky Documentation"
      description="Plain introductions to four experimental computing ideas used in Apocky source code, followed by technical examples."
    >
      <h1 className="docs-h1">Technical foundations</h1>
      <p className="docs-blurb">
        Four experimental computing ideas used in project source code, explained before their technical names.
      </p>

      <p className="docs-p">
        In these projects, <strong>substrate</strong> is an engineering label for shared low-level code and
        ideas. It means “the computing foundation underneath other parts of the program.” It does not mean the
        software is a person, a living organism, or an independently operating service.
      </p>

      <Callout kind="warn" title="Implementation status">
        Source code, tests, a design specification, and a released product are different forms of evidence.
        This page explains the intended architecture. It does not claim that every item below is active in the
        public test build or that every privacy property has been independently verified.
      </Callout>

      <h2 className="docs-h2">A coordinate-based state map (ω-field)</h2>
      <p className="docs-p">
        The project uses <strong>ω-field</strong> as a name for an experimental way of organizing simulation
        data by coordinates and relationships. The Greek letter ω is pronounced “omega.” A technical document
        may call this an <em>addressable manifold</em>; in plain language, that means a structured space in
        which each piece of data has a location.
      </p>
      <p className="docs-p">
        The design aims to keep one clearly identified source of simulation state and derive displays or other
        views from it. Whether a particular game scene uses this system is a release-specific implementation
        question, not something the name itself proves.
      </p>

      <h2 className="docs-h2">Permissions stored as bits (Σ-mask)</h2>
      <p className="docs-p">
        A <strong>bitmask</strong> is a number whose individual bits act like small on-or-off switches.
        <strong>Σ-mask</strong> (pronounced “sigma mask”) is the project name for using such switches to
        represent permissions in code.
      </p>
      <p className="docs-p">
        The design goal is to check a narrow permission before a protected read, write, or network action.
        A mask in source code is not, by itself, evidence that a complete user-facing consent flow exists.
      </p>

      <Callout kind="note" title="Permission claims must be tested">
        For the current distinction between website choices, LoA release statements, and planned controls, see{' '}
        <a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>Permissions and data sharing</a>.
      </Callout>

      <h2 className="docs-h2">A compact learning method (KAN)</h2>
      <p className="docs-p">
        <strong>KAN</strong> stands for <strong>Kolmogorov-Arnold Network</strong>, a type of mathematical
        model. The project explores KANs for small, bounded choices in game generation. The intended advantages
        are:
      </p>
      <ul className="docs-ul">
        <li>Small models can be inexpensive to run.</li>
        <li>Individual mathematical functions can be inspected and graphed.</li>
        <li>A fixed rule can remain available when a learned choice is unavailable or rejected.</li>
      </ul>
      <p className="docs-p">
        Technical notes call those bounded choices <strong>swap points</strong>. A swap point is simply a place
        where one selection method can be exchanged for another while keeping a fixed fallback.
      </p>

      <h2 className="docs-h2">High-dimensional computing (HDC)</h2>
      <p className="docs-p">
        <strong>HDC</strong> stands for Hyperdimensional Computing. Cells signal to one another with
        long patterns of numbers called vectors. Operations can combine those patterns and later compare or
        separate them. The project explores HDC as a compact way to label and relate information.
      </p>
      <p className="docs-p">
        Architecture documents also propose using HDC in the planned Mycelium network. That proposed use is not
        a statement that a public multiplayer network is currently running.
      </p>

      <h2 className="docs-h2">Why the design combines them</h2>
      <p className="docs-p">
        The architectural goal is to combine organized simulation state, explicit permissions, bounded learning,
        and compact representations without forcing every project into unrelated databases or services.
      </p>
      <p className="docs-p">
        Several source libraries use these names. Their presence in a repository establishes that code exists;
        it does not alone establish product integration, performance, security, or deployment.
      </p>

      <h2 className="docs-h2">Technical example in CSSL</h2>
      <p className="docs-p">
        The example below shows how technical source material proposes calling host libraries from CSSL.
        <code className="docs-ic"> extern "C"</code> means that CSSL expects a compatible function supplied by
        another compiled library. This is reference material; you can skip it without losing the explanation above.
      </p>

      <CodeBlock lang="cssl" caption="The four primitives in one scene-tick · CSSL-authored">{`module com.apocky.loa.systems.substrate_demo

// § ω-field · address a cell, read its value
extern "C" fn omega_field_read(handle: u32, x: i32, y: i32, z: i32, out: u64) -> u32 ;
extern "C" fn omega_field_write(handle: u32, x: i32, y: i32, z: i32, value: u64) -> u32 ;

// § Σ-mask · check sovereign-cap before a cross-process emit
extern "C" fn sigma_mask_check(handle: u32, cell_addr: u64, observer: u64) -> u32 ;

// § KAN · classify an affix template at a substrate "swap point" (SP-PG-1..5)
extern "C" fn kan_classify_swap_point(handle: u32, swap_id: u32,
                                      hist_hash: u64, bias_out: u64) -> u32 ;

// § HDC · bind a tag to a cell-payload, broadcast over the mycelium edge
extern "C" fn hdc_bind_emit(handle: u32, cell_addr: u64,
                            tag_ptr: u64, tag_len: u32) -> u32 ;

fn substrate_tick(handle: u32, observer: u64) -> u32 {
    let cell_x: i32 = 12 ;
    let cell_y: i32 = 0 ;
    let cell_z: i32 = -7 ;

    // 1. Σ-mask gate FIRST — substrate refuses cross-observer reads without cap
    let cell_addr: u64 = pack_addr(cell_x, cell_y, cell_z) ;
    let mask_status: u32 = sigma_mask_check(handle, cell_addr, observer) ;
    if mask_status != 0 { return 128 ; }    // 128.. = sovereign-cap denied

    // 2. ω-field read — only after Σ-mask grants cap
    let mut value: u64 = 0 ;
    let read_status: u32 = omega_field_read(handle, cell_x, cell_y, cell_z, &mut value as u64) ;
    if read_status != 0 { return read_status ; }

    // 3. KAN-driven swap-point bias for procgen affixes (SP-PG-4 = loot-affix)
    let mut bias: u64 = 0 ;
    let kan_status: u32 = kan_classify_swap_point(handle, 4, value, &mut bias as u64) ;
    if kan_status != 0 { return kan_status ; }

    // 4. HDC tag-broadcast — substrate talks to itself via mycelium edges
    let tag: u64 = bias ;
    let tag_len: u32 = 8 ;
    hdc_bind_emit(handle, cell_addr, &tag as u64, tag_len)
}

fn pack_addr(x: i32, y: i32, z: i32) -> u64 {
    let xu: u64 = (x as u64) & 0xfffff ;
    let yu: u64 = (y as u64) & 0xfffff ;
    let zu: u64 = (z as u64) & 0xfffff ;
    (xu << 40) | (yu << 20) | zu
}`}</CodeBlock>

      <Callout kind="note" title="Technical scope">
        The example demonstrates an intended interface. For an introduction to the programming language, see{' '}
        <a href="/docs/cssl-language" style={{ color: '#7dd3fc' }}>CSSL language overview</a>.
      </Callout>

      <h2 className="docs-h2">Where to read more</h2>
      <ul className="docs-ul">
        <li><a href="/words" style={{ color: '#7dd3fc' }}>Definitions for technical words and symbols</a></li>
        <li><a href="/docs/sovereignty" style={{ color: '#7dd3fc' }}>Permissions and data sharing</a></li>
        <li><a href="/docs/mycelium" style={{ color: '#7dd3fc' }}>The planned Mycelium design</a></li>
        <li>Technical source: <code className="docs-ic">specs/grand-vision/15_UNIFIED_SUBSTRATE.csl</code></li>
      </ul>

      <PrevNextNav slug="substrate" />
    </DocsLayout>
  );
};

export default Page;
