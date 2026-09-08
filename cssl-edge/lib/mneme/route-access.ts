import type { NextApiRequest, NextApiResponse } from 'next';

import {
  MNEME_CAPABILITIES,
  requireMnemeProfileAccess,
  type MnemeCapability,
} from './auth';
import {
  MNEME_MEMBER_ROUTE_PROFILE,
  requireMnemeMemberProfile,
  respondMnemeMemberFailure,
  setMnemePrivateHeaders,
} from './member-profile';

export interface MnemeRouteAccess {
  readonly actor: 'member' | 'service';
  readonly capability: MnemeCapability;
  readonly profileId: string;
}

/**
 * Keep the public member namespace opaque while retaining the authenticated,
 * capability-bound service rail used by Apocrypha's memory workers.
 */
export async function requireMnemeRouteAccess<T>(
  req: NextApiRequest,
  res: NextApiResponse<T>,
  capability: MnemeCapability,
): Promise<MnemeRouteAccess | null> {
  setMnemePrivateHeaders(res);
  const requested = Array.isArray(req.query.profile) ? req.query.profile[0] : req.query.profile;
  if (requested === MNEME_MEMBER_ROUTE_PROFILE) {
    const member = await requireMnemeMemberProfile(req);
    if (!member.ok) {
      respondMnemeMemberFailure(res, member);
      return null;
    }
    return { actor: 'member', capability, profileId: member.profileId };
  }

  const service = await requireMnemeProfileAccess(req, res, capability);
  if (!service) return null;
  return { actor: service.actor, capability, profileId: service.profileId };
}

export { MNEME_CAPABILITIES };
