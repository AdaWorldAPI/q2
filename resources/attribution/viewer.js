// Auto-injected by AttributionViewerTransform when --attribution=git
// (or YAML attribution: git) is active. Paints each attributed
// element in its author's colour (descendants inherit via the
// cascade), then binds delegated mouseover / mouseout listeners on
// `[data-attr-actor]` elements that surface a floating badge built
// from each element's data-attr-* attributes.

(function () {
    // Paint each attributed element in its author's colour. Matches
    // the hub-client's `AttributionWrap`, which sets the same colour
    // inline on the React side.
    document.querySelectorAll('[data-attr-actor]').forEach(function (el) {
        var color = el.getAttribute('data-attr-color');
        if (color) {
            el.style.color = color;
            el.style.textDecorationColor = color;
        }
    });

    function formatRelativeTime(timestamp) {
        var now = Date.now();
        // git blame emits seconds, Automerge emits milliseconds;
        // the 1e12 threshold distinguishes them.
        var tsMs = timestamp < 1e12 ? timestamp * 1000 : timestamp;
        var diffSec = Math.floor((now - tsMs) / 1000);
        if (diffSec < 60) return 'just now';
        var diffMin = Math.floor(diffSec / 60);
        if (diffMin < 60) return diffMin + 'm ago';
        var diffHr = Math.floor(diffMin / 60);
        if (diffHr < 24) return diffHr + 'h ago';
        var diffDay = Math.floor(diffHr / 24);
        return diffDay + 'd ago';
    }

    function buildBadge(leaf) {
        var name = leaf.getAttribute('data-attr-name');
        var color = leaf.getAttribute('data-attr-color');
        var time = Number(leaf.getAttribute('data-attr-time'));
        if (!name || !color || !Number.isFinite(time)) return null;

        var badge = document.createElement('span');
        badge.className = 'q2-attr-badge';
        badge.style.setProperty('--attr-color', color);

        var dot = document.createElement('span');
        dot.className = 'q2-attr-badge-dot';
        dot.style.backgroundColor = color;
        badge.appendChild(dot);

        badge.appendChild(document.createTextNode(name + ' '));

        var timeEl = document.createElement('span');
        timeEl.className = 'q2-attr-badge-time';
        timeEl.textContent = formatRelativeTime(time);
        badge.appendChild(timeEl);

        return badge;
    }

    var currentBadge = null;

    document.addEventListener('mouseover', function (e) {
        var leaf = e.target.closest('[data-attr-actor]');
        if (!leaf) return;
        if (currentBadge) currentBadge.remove();

        var badge = buildBadge(leaf);
        if (!badge) return;

        var rect = leaf.getBoundingClientRect();
        badge.style.position = 'fixed';
        badge.style.top = (rect.bottom + 2) + 'px';
        badge.style.left = rect.left + 'px';

        document.body.appendChild(badge);
        currentBadge = badge;
    });

    document.addEventListener('mouseout', function (e) {
        var related = e.relatedTarget;
        if (related && related.closest && related.closest('[data-attr-actor]')) {
            return;
        }
        if (currentBadge) {
            currentBadge.remove();
            currentBadge = null;
        }
    });
})();
