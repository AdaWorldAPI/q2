import { useContext } from 'react';
import { RegistryContext } from '../framework/RegistryContext';
import { useNodeAttribution } from '../framework/AttributionLookupContext';
import type { BlockNode, InlineNode, NodeArgs } from '../framework/types';
import { blockStyle, inlineStyle } from './styles';

/**
 * q2-debug Block dispatcher: looks up the format registry by Pandoc tag
 * and renders the corresponding leaf component, falling back to a bordered
 * "Not registered" message when no component is registered for the tag.
 *
 * Phase 5c — when attribution is on for this node (resolved by
 * `useNodeAttribution`), wrap the dispatched output in a
 * `.q2-attr-wrap` div carrying `data-sid` + inline `color` so the
 * descendant text inherits the author's identity colour. Off path
 * the wrap is skipped, leaving the dispatcher output byte-identical
 * to pre-attribution.
 */
export const Block = (args: NodeArgs<BlockNode>) => {
    const { registry } = useContext(RegistryContext);
    const attribution = useNodeAttribution(args.node as unknown as { s?: number });

    const Component = registry[args.node.t];
    const inner = Component
        ? <Component {...args} />
        : <div style={blockStyle}><strong>Not registered: {args.node.t}</strong></div>;

    if (!attribution) return inner;
    const sid = (args.node as unknown as { s?: number }).s;
    return (
        <div className="q2-attr-wrap" data-sid={sid} style={{ color: attribution.color }}>
            {inner}
        </div>
    );
}

/**
 * q2-debug Inline dispatcher: same as Block but for inline-level nodes,
 * with the inline-flavored "Not registered" miss path and a `<span>`
 * attribution wrap.
 */
export const Inline = (args: NodeArgs<InlineNode>) => {
    const { registry } = useContext(RegistryContext);
    const attribution = useNodeAttribution(args.node as unknown as { s?: number });

    const Component = registry[args.node.t];
    const inner = Component
        ? <Component {...args} />
        : <span style={inlineStyle}><strong>Not registered: {args.node.t}</strong></span>;

    if (!attribution) return inner;
    const sid = (args.node as unknown as { s?: number }).s;
    return (
        <span className="q2-attr-wrap" data-sid={sid} style={{ color: attribution.color }}>
            {inner}
        </span>
    );
}
