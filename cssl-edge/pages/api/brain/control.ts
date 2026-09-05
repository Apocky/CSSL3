import { createRemoteControlHandler } from '@/lib/brain/remote-control';

export const config = { api: { bodyParser: { sizeLimit: '256kb' }, responseLimit: '512kb' }, maxDuration: 300 };
export default createRemoteControlHandler();
