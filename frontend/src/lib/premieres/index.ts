/**
 * Each event's premiere, by its slug. An event without one gets `default`.
 * The `/event` skill adds a line here with the event's own choreography.
 */

import * as avengersDoomsday from './avengers-doomsday';
import * as fallback from './default';
import type { Cast, Stage, Story } from './stage';

export type Choreography = (stage: Stage, cast: Cast, story: Story) => { handoff: number };

const OWN: Record<string, Choreography> = {
	'avengers-doomsday': avengersDoomsday.play
};

export function choreographyFor(slug: string): Choreography {
	return OWN[slug] ?? fallback.play;
}
