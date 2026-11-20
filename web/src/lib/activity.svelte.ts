// Health › Activity's jobs (RN; administrators', as Health is): read once, then followed on the live channel (`activity`: a patch
// per change), never polled. Its rows are lib/activityRows'.
import { getActivity } from './api';
import { on } from './events.svelte';
import { me } from './me.svelte';
import { patched, type ActivityJob } from './activityRows';

export const activity = $state<{ jobs: ActivityJob[] | null }>({ jobs: null });
let listening = false;

export function loadActivity() {
  if (!me.admin) return;
  if (!listening) {
    listening = true;
    on<ActivityJob>('activity', (j) => (activity.jobs = patched(activity.jobs ?? [], j)));
  }
  getActivity().then(
    (r) => (activity.jobs = r.jobs),
    () => {},
  );
}
