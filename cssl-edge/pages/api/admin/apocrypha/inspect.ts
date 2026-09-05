import { createOperatorInspectionHandler } from '@/lib/mobile/operator-inspection';

export const config = { api: { bodyParser: { sizeLimit: '4kb' }, responseLimit: '512kb' }, maxDuration: 120 };
export default createOperatorInspectionHandler();
