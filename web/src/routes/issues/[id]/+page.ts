import type { PageLoad } from './$types';

export const load: PageLoad = ({ params }) => {
  const id = Number.parseInt(params.id, 10);
  return { id: Number.isFinite(id) ? id : null };
};
