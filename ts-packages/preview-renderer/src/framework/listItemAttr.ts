import type { Attr } from './types';

/**
 * Compose the DOM props for a list item's `<li>` from an optional hoisted block
 * attribute (`itemAttr[i]`, see bd-aeyss6p5) and the incremental `fragment`
 * class.
 *
 * `fragment` is prepended (so theme rules keying off `.fragment` keep working),
 * then the item's own classes; `id` and `data-*`/`role` come from the attr.
 * This mirrors the Rust `composed_list_item_attr` HTML writer so `q2 preview`
 * matches `q2 render`.
 */
export function liItemAttrProps(
    attr: Attr | null | undefined,
    fragment: boolean,
): Record<string, string> {
    const props: Record<string, string> = {};
    const classes: string[] = [];
    if (fragment) classes.push('fragment');
    if (attr) {
        const [id, attrClasses, kvs] = attr;
        if (id) props.id = id;
        for (const c of attrClasses) {
            if (!classes.includes(c)) classes.push(c);
        }
        for (const [k, v] of kvs) {
            if (k.startsWith('data-') || k === 'role') props[k] = v;
        }
    }
    if (classes.length) props.className = classes.join(' ');
    return props;
}
