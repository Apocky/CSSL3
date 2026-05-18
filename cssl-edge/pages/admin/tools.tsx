// /admin/tools · Apocrypha's registered tool registry.
//
// The 19+ Tier-0 sub-mind interfaces (per spec 13) across 7 organs:
// memory / swarm / language / forage / evolve / dream / state. Filterable by
// category + permission tier.

import type { NextPage } from 'next';
import { useEffect, useMemo, useState } from 'react';

import AdminLayout from '../../components/AdminLayout';
import { listTools, type ToolInfo } from '../../lib/apocrypha/client';

const TIER_COLOR: Record<string, string> = {
  PUBLIC: '#7fd17f',
  USER: '#7dd3fc',
  ADMIN: '#fbbf24',
  NUCLEAR: '#ff8888',
};

const Tools: NextPage = () => {
  const [adminAuthorized, setAdminAuthorized] = useState(false);
  const [tools, setTools] = useState<ToolInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState('');
  const [categoryFilter, setCategoryFilter] = useState('all');

  useEffect(() => {
    if (!adminAuthorized) return;
    listTools()
      .then((r) => setTools(r.tools))
      .catch((err) => setError(err instanceof Error ? err.message : String(err)))
      .finally(() => setLoading(false));
  }, [adminAuthorized]);

  const categories = useMemo(() => {
    const s = new Set(tools.map((t) => t.category));
    return ['all', ...Array.from(s).sort()];
  }, [tools]);

  const filtered = useMemo(() => {
    const f = filter.trim().toLowerCase();
    return tools.filter((t) => {
      if (f && !t.name.toLowerCase().includes(f) && !t.description.toLowerCase().includes(f))
        return false;
      if (categoryFilter !== 'all' && t.category !== categoryFilter) return false;
      return true;
    });
  }, [tools, filter, categoryFilter]);

  return (
    <AdminLayout title="⊑ Tools" onAdminCheck={(c) => setAdminAuthorized(c.authorized)}>
      {!adminAuthorized ? (
        <div style={{ padding: '2rem', color: '#a0a0b0' }}>
          <p>Tool registry requires admin authentication.</p>
        </div>
      ) : (
        <div style={{ display: 'flex', flexDirection: 'column', gap: '0.8rem', color: '#cdd6e4' }}>
          <p style={{ fontSize: '0.82rem', color: '#7a7a8c', marginTop: 0 }}>
            Tier-0 sub-mind interfaces (per spec 13 §TOOL-REGISTRY). Each tool is a callable
            capability Apocrypha can invoke during its tool-use loop. Default-deny per I-28 :
            tools are NOT callable until Apocky-granted (status indicator per row).
          </p>

          <div style={{
            display: 'flex',
            gap: '0.5rem',
            padding: '0.5rem',
            border: '1px solid #2a2a3a',
            borderRadius: 6,
            background: 'rgba(15, 15, 22, 0.5)',
          }}>
            <input
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              placeholder="filter by name or description…"
              title="Substring match on name + description"
              style={{
                flex: 1,
                background: '#0a0a10',
                border: '1px solid #2a2a3a',
                color: '#cdd6e4',
                padding: '0.4rem 0.6rem',
                fontSize: '0.85rem',
                borderRadius: 3,
                fontFamily: 'inherit',
              }}
            />
            <select
              value={categoryFilter}
              onChange={(e) => setCategoryFilter(e.target.value)}
              title="Filter by tool category (organ)"
              style={{
                background: '#0a0a10',
                border: '1px solid #2a2a3a',
                color: '#cdd6e4',
                padding: '0.4rem 0.6rem',
                fontSize: '0.85rem',
                borderRadius: 3,
                fontFamily: 'inherit',
              }}
            >
              {categories.map((c) => (
                <option key={c} value={c}>{c}</option>
              ))}
            </select>
            <span title="Filtered / Total" style={{
              fontSize: '0.8rem',
              color: '#7a7a8c',
              alignSelf: 'center',
              padding: '0 0.5rem',
            }}>
              {filtered.length}/{tools.length}
            </span>
          </div>

          {loading && <div style={{ color: '#7a7a8c' }}>§ loading…</div>}
          {error && (
            <div style={{ color: '#ff8888', fontSize: '0.85rem' }}>error : {error}</div>
          )}

          <div style={{
            display: 'grid',
            gridTemplateColumns: 'repeat(auto-fill, minmax(360px, 1fr))',
            gap: '0.6rem',
          }}>
            {filtered.map((t) => (
              <article key={t.name}
                       title={`${t.permission_tier}-tier · ${t.independent ? 'parallelizable' : 'serial'} · ${t.timeout_s}s timeout`}
                       style={{
                         padding: '0.7rem 0.85rem',
                         border: '1px solid #1f1f2a',
                         borderRadius: 6,
                         background: 'rgba(20, 20, 30, 0.4)',
                         fontSize: '0.82rem',
                       }}>
                <div style={{
                  display: 'flex',
                  gap: '0.4rem',
                  alignItems: 'baseline',
                  marginBottom: '0.4rem',
                }}>
                  <span style={{ fontWeight: 600, color: '#cdd6e4' }}>{t.name}</span>
                  <span style={{
                    marginLeft: 'auto',
                    fontSize: '0.65rem',
                    padding: '0.1rem 0.35rem',
                    borderRadius: 3,
                    background: 'rgba(15, 15, 22, 0.6)',
                    color: TIER_COLOR[t.permission_tier] ?? '#9aa0a6',
                    border: `1px solid ${TIER_COLOR[t.permission_tier] ?? '#9aa0a6'}33`,
                  }}>
                    {t.permission_tier}
                  </span>
                </div>
                <div style={{ color: '#9aa0a6', lineHeight: 1.45 }}>
                  {t.description}
                </div>
                <div style={{
                  marginTop: '0.4rem',
                  display: 'flex',
                  gap: '0.4rem',
                  fontSize: '0.7rem',
                  color: '#7a7a8c',
                  flexWrap: 'wrap',
                }}>
                  <span title="Tool category (organ)">{t.category}</span>
                  <span>·</span>
                  <span title="Per-call wall-clock budget">{t.timeout_s}s</span>
                  {t.independent && (<><span>·</span><span title="May run in parallel">parallel</span></>)}
                  {t.accepts_hv && (<><span>·</span><span title="Accepts HRR hypervector payloads (in-process only ; not MCP-exposed)">hrr-native</span></>)}
                </div>
              </article>
            ))}
          </div>
        </div>
      )}
    </AdminLayout>
  );
};

export default Tools;
