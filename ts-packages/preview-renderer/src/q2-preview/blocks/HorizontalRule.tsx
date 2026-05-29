import { dataLocProps } from '../../framework';
import type { HorizontalRuleBlock, NodeArgs } from '../../framework';

export const HorizontalRule = (args: NodeArgs<HorizontalRuleBlock>) => (
    <hr {...dataLocProps(args.node)} />
);
