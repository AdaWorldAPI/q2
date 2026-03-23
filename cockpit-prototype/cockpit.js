/* ================================================================
   q2 Graph Notebook — Cockpit Interactivity
   Force-directed graph, sortable table, linked selection
   ================================================================ */

// ---- Data Model ----
const NODES = [
  { id: 'srv-001', label: 'web-server-01',   type: 'Server',   region: 'us-east-1', status: 'healthy',  cpu: 0.67, memory: 28.4, connections: 5  },
  { id: 'srv-002', label: 'web-server-02',   type: 'Server',   region: 'us-east-1', status: 'healthy',  cpu: 0.54, memory: 24.1, connections: 4  },
  { id: 'srv-003', label: 'web-server-03',   type: 'Server',   region: 'eu-west-1', status: 'healthy',  cpu: 0.42, memory: 31.2, connections: 5  },
  { id: 'srv-004', label: 'web-server-04',   type: 'Server',   region: 'eu-west-1', status: 'warning',  cpu: 0.81, memory: 29.8, connections: 3  },
  { id: 'api-001', label: 'api-gateway-01',  type: 'Gateway',  region: 'us-east-1', status: 'healthy',  cpu: 0.31, memory: 8.2,  connections: 8  },
  { id: 'api-002', label: 'api-gateway-02',  type: 'Gateway',  region: 'eu-west-1', status: 'healthy',  cpu: 0.28, memory: 7.9,  connections: 7  },
  { id: 'db-001',  label: 'db-postgres-01',  type: 'Database',  region: 'us-east-1', status: 'healthy',  cpu: 0.45, memory: 62.3, connections: 6  },
  { id: 'db-002',  label: 'db-postgres-02',  type: 'Database',  region: 'eu-west-1', status: 'healthy',  cpu: 0.38, memory: 58.7, connections: 5  },
  { id: 'cache-01',label: 'cache-redis-01',  type: 'Cache',    region: 'us-east-1', status: 'healthy',  cpu: 0.12, memory: 16.0, connections: 6  },
  { id: 'cache-02',label: 'cache-redis-02',  type: 'Cache',    region: 'eu-west-1', status: 'healthy',  cpu: 0.09, memory: 16.0, connections: 5  },
  { id: 'lb-001',  label: 'lb-haproxy-01',   type: 'LoadBalancer', region: 'us-east-1', status: 'healthy', cpu: 0.22, memory: 4.1, connections: 6  },
  { id: 'lb-002',  label: 'lb-haproxy-02',   type: 'LoadBalancer', region: 'eu-west-1', status: 'healthy', cpu: 0.19, memory: 3.8, connections: 5  },
  { id: 'mon-001', label: 'prometheus-01',   type: 'Monitor',  region: 'us-east-1', status: 'healthy',  cpu: 0.55, memory: 12.4, connections: 10 },
  { id: 'msg-001', label: 'kafka-broker-01', type: 'Queue',    region: 'us-east-1', status: 'healthy',  cpu: 0.61, memory: 32.0, connections: 8  },
  { id: 'msg-002', label: 'kafka-broker-02', type: 'Queue',    region: 'eu-west-1', status: 'warning',  cpu: 0.78, memory: 30.5, connections: 7  },
  { id: 'cdn-001', label: 'cdn-edge-01',     type: 'CDN',      region: 'global',    status: 'healthy',  cpu: 0.15, memory: 2.1,  connections: 4  },
  { id: 'dns-001', label: 'dns-resolver-01', type: 'DNS',      region: 'global',    status: 'healthy',  cpu: 0.08, memory: 1.2,  connections: 3  },
  { id: 'vault-01',label: 'vault-01',        type: 'Secrets',  region: 'us-east-1', status: 'healthy',  cpu: 0.05, memory: 2.0,  connections: 4  },
  { id: 'log-001', label: 'elasticsearch-01',type: 'Search',   region: 'us-east-1', status: 'healthy',  cpu: 0.72, memory: 48.0, connections: 5  },
  { id: 'svc-001', label: 'auth-service-01', type: 'Service',  region: 'us-east-1', status: 'healthy',  cpu: 0.33, memory: 8.8,  connections: 5  },
  { id: 'svc-002', label: 'user-service-01', type: 'Service',  region: 'us-east-1', status: 'healthy',  cpu: 0.29, memory: 7.2,  connections: 4  },
  { id: 'svc-003', label: 'order-service-01',type: 'Service',  region: 'eu-west-1', status: 'critical', cpu: 0.92, memory: 14.5, connections: 6  },
  { id: 'wrk-001', label: 'worker-batch-01', type: 'Worker',   region: 'us-east-1', status: 'healthy',  cpu: 0.48, memory: 16.3, connections: 3  },
  { id: 'wrk-002', label: 'worker-batch-02', type: 'Worker',   region: 'eu-west-1', status: 'healthy',  cpu: 0.44, memory: 15.9, connections: 3  },
];

const EDGES = [
  { source: 'lb-001',  target: 'srv-001', label: 'ROUTES_TO' },
  { source: 'lb-001',  target: 'srv-002', label: 'ROUTES_TO' },
  { source: 'lb-002',  target: 'srv-003', label: 'ROUTES_TO' },
  { source: 'lb-002',  target: 'srv-004', label: 'ROUTES_TO' },
  { source: 'srv-001', target: 'api-001', label: 'SERVES' },
  { source: 'srv-002', target: 'api-001', label: 'SERVES' },
  { source: 'srv-003', target: 'api-002', label: 'SERVES' },
  { source: 'srv-004', target: 'api-002', label: 'SERVES' },
  { source: 'api-001', target: 'svc-001', label: 'CALLS' },
  { source: 'api-001', target: 'svc-002', label: 'CALLS' },
  { source: 'api-002', target: 'svc-003', label: 'CALLS' },
  { source: 'svc-001', target: 'db-001',  label: 'QUERIES' },
  { source: 'svc-002', target: 'db-001',  label: 'QUERIES' },
  { source: 'svc-003', target: 'db-002',  label: 'QUERIES' },
  { source: 'srv-001', target: 'cache-01',label: 'READS_FROM' },
  { source: 'srv-002', target: 'cache-01',label: 'READS_FROM' },
  { source: 'srv-003', target: 'cache-02',label: 'READS_FROM' },
  { source: 'srv-004', target: 'cache-02',label: 'READS_FROM' },
  { source: 'msg-001', target: 'wrk-001', label: 'DELIVERS' },
  { source: 'msg-002', target: 'wrk-002', label: 'DELIVERS' },
  { source: 'svc-003', target: 'msg-002', label: 'PUBLISHES' },
  { source: 'svc-002', target: 'msg-001', label: 'PUBLISHES' },
  { source: 'mon-001', target: 'srv-001', label: 'MONITORS' },
  { source: 'mon-001', target: 'srv-002', label: 'MONITORS' },
  { source: 'mon-001', target: 'srv-003', label: 'MONITORS' },
  { source: 'mon-001', target: 'srv-004', label: 'MONITORS' },
  { source: 'mon-001', target: 'db-001',  label: 'MONITORS' },
  { source: 'mon-001', target: 'db-002',  label: 'MONITORS' },
  { source: 'dns-001', target: 'cdn-001', label: 'RESOLVES' },
  { source: 'cdn-001', target: 'lb-001',  label: 'FORWARDS' },
  { source: 'cdn-001', target: 'lb-002',  label: 'FORWARDS' },
];

// ---- Type → Color mapping ----
const TYPE_COLORS = {
  Server:       '#00bcd4',
  Gateway:      '#4dd0e1',
  Database:     '#ffc107',
  Cache:        '#ff7043',
  LoadBalancer: '#66bb6a',
  Monitor:      '#ab47bc',
  Queue:        '#ef5350',
  CDN:          '#42a5f5',
  DNS:          '#78909c',
  Secrets:      '#8d6e63',
  Search:       '#7e57c2',
  Service:      '#26c6da',
  Worker:       '#9ccc65',
};

// ---- State ----
let selectedNodeId = null;
let simulation = null;
let svgWidth = 800;
let svgHeight = 500;

// Deep-copy nodes for simulation (add x, y, vx, vy)
const simNodes = NODES.map(n => ({ ...n, x: 0, y: 0, vx: 0, vy: 0 }));
const simEdges = EDGES.map(e => ({
  ...e,
  sourceNode: simNodes.find(n => n.id === e.source),
  targetNode: simNodes.find(n => n.id === e.target),
}));

// ---- Force Simulation (no d3 dependency) ----
function initSimulation() {
  const svg = document.getElementById('graphSvg');
  const rect = svg.parentElement.getBoundingClientRect();
  svgWidth = rect.width || 800;
  svgHeight = rect.height || 500;

  // Initialize positions in a circle
  const cx = svgWidth / 2;
  const cy = svgHeight / 2;
  const r = Math.min(svgWidth, svgHeight) * 0.35;
  simNodes.forEach((n, i) => {
    const angle = (2 * Math.PI * i) / simNodes.length;
    n.x = cx + r * Math.cos(angle);
    n.y = cy + r * Math.sin(angle);
    n.vx = 0;
    n.vy = 0;
  });

  // Run simulation
  let alpha = 1;
  const decay = 0.005;
  const minAlpha = 0.001;

  function tick() {
    if (alpha < minAlpha) { renderGraph(); return; }

    // Center gravity
    simNodes.forEach(n => {
      n.vx += (cx - n.x) * 0.001 * alpha;
      n.vy += (cy - n.y) * 0.001 * alpha;
    });

    // Repulsion (charge)
    for (let i = 0; i < simNodes.length; i++) {
      for (let j = i + 1; j < simNodes.length; j++) {
        const a = simNodes[i], b = simNodes[j];
        let dx = b.x - a.x, dy = b.y - a.y;
        let dist = Math.sqrt(dx * dx + dy * dy) || 1;
        const force = -300 * alpha / (dist * dist);
        const fx = (dx / dist) * force;
        const fy = (dy / dist) * force;
        a.vx -= fx; a.vy -= fy;
        b.vx += fx; b.vy += fy;
      }
    }

    // Spring (links)
    simEdges.forEach(e => {
      const s = e.sourceNode, t = e.targetNode;
      if (!s || !t) return;
      let dx = t.x - s.x, dy = t.y - s.y;
      let dist = Math.sqrt(dx * dx + dy * dy) || 1;
      const desiredLen = 120;
      const force = (dist - desiredLen) * 0.005 * alpha;
      const fx = (dx / dist) * force;
      const fy = (dy / dist) * force;
      s.vx += fx; s.vy += fy;
      t.vx -= fx; t.vy -= fy;
    });

    // Velocity damping and position update
    simNodes.forEach(n => {
      if (n._dragging) return;
      n.vx *= 0.6;
      n.vy *= 0.6;
      n.x += n.vx;
      n.y += n.vy;
      // Bounds
      n.x = Math.max(40, Math.min(svgWidth - 40, n.x));
      n.y = Math.max(40, Math.min(svgHeight - 40, n.y));
    });

    alpha -= decay;
    renderGraph();
    requestAnimationFrame(tick);
  }

  requestAnimationFrame(tick);
}

// ---- SVG Rendering ----
const NS = 'http://www.w3.org/2000/svg';

function renderGraph() {
  const svg = document.getElementById('graphSvg');
  svg.innerHTML = '';

  // Edges
  simEdges.forEach(e => {
    const s = e.sourceNode, t = e.targetNode;
    if (!s || !t) return;

    const isHighlighted = selectedNodeId && (s.id === selectedNodeId || t.id === selectedNodeId);

    const line = document.createElementNS(NS, 'line');
    line.setAttribute('x1', s.x);
    line.setAttribute('y1', s.y);
    line.setAttribute('x2', t.x);
    line.setAttribute('y2', t.y);
    line.setAttribute('class', 'graph-link' + (isHighlighted ? ' highlighted' : ''));
    svg.appendChild(line);
  });

  // Nodes
  simNodes.forEach(n => {
    const g = document.createElementNS(NS, 'g');
    g.setAttribute('class', 'graph-node' + (n.id === selectedNodeId ? ' selected' : ''));
    g.setAttribute('data-id', n.id);

    const color = TYPE_COLORS[n.type] || '#00bcd4';
    const radius = 6 + Math.min(n.connections, 12) * 1.2;

    // Glow
    if (n.id === selectedNodeId) {
      const glow = document.createElementNS(NS, 'circle');
      glow.setAttribute('cx', n.x);
      glow.setAttribute('cy', n.y);
      glow.setAttribute('r', radius + 6);
      glow.setAttribute('fill', 'none');
      glow.setAttribute('stroke', color);
      glow.setAttribute('stroke-width', '1');
      glow.setAttribute('opacity', '0.3');
      g.appendChild(glow);
    }

    const circle = document.createElementNS(NS, 'circle');
    circle.setAttribute('cx', n.x);
    circle.setAttribute('cy', n.y);
    circle.setAttribute('r', radius);
    circle.setAttribute('fill', color);
    circle.setAttribute('fill-opacity', n.id === selectedNodeId ? '1' : '0.7');
    circle.setAttribute('stroke', n.id === selectedNodeId ? '#e8eaf6' : color);
    circle.setAttribute('stroke-width', n.id === selectedNodeId ? '2.5' : '1.5');
    circle.setAttribute('stroke-opacity', '0.8');
    g.appendChild(circle);

    // Label
    const text = document.createElementNS(NS, 'text');
    text.setAttribute('x', n.x);
    text.setAttribute('y', n.y + radius + 14);
    text.setAttribute('text-anchor', 'middle');
    text.textContent = n.label;
    g.appendChild(text);

    // Events
    g.addEventListener('mousedown', (ev) => startDrag(ev, n));
    g.addEventListener('click', () => selectNode(n.id));

    svg.appendChild(g);
  });
}

// ---- Drag ----
let dragNode = null;
let dragOffset = { x: 0, y: 0 };

function startDrag(ev, node) {
  ev.preventDefault();
  dragNode = node;
  node._dragging = true;
  const svg = document.getElementById('graphSvg');
  const rect = svg.getBoundingClientRect();
  dragOffset.x = ev.clientX - rect.left - node.x;
  dragOffset.y = ev.clientY - rect.top - node.y;
}

document.addEventListener('mousemove', (ev) => {
  if (!dragNode) return;
  const svg = document.getElementById('graphSvg');
  const rect = svg.getBoundingClientRect();
  dragNode.x = ev.clientX - rect.left - dragOffset.x;
  dragNode.y = ev.clientY - rect.top - dragOffset.y;
  renderGraph();
});

document.addEventListener('mouseup', () => {
  if (dragNode) {
    dragNode._dragging = false;
    dragNode = null;
  }
});

// ---- Selection (linked: graph ↔ table ↔ properties) ----
function selectNode(id) {
  selectedNodeId = (selectedNodeId === id) ? null : id;
  renderGraph();
  renderTable();
  renderProperties();
}

// ---- Result Table ----
let sortCol = null;
let sortAsc = true;

function renderTable() {
  const tbody = document.getElementById('resultBody');
  tbody.innerHTML = '';

  let sorted = [...NODES];
  if (sortCol) {
    sorted.sort((a, b) => {
      let va = a[sortCol], vb = b[sortCol];
      if (typeof va === 'string') va = va.toLowerCase();
      if (typeof vb === 'string') vb = vb.toLowerCase();
      if (va < vb) return sortAsc ? -1 : 1;
      if (va > vb) return sortAsc ? 1 : -1;
      return 0;
    });
  }

  sorted.forEach(n => {
    const tr = document.createElement('tr');
    tr.className = n.id === selectedNodeId ? 'selected' : '';
    tr.addEventListener('click', () => selectNode(n.id));

    const statusClass = n.status === 'healthy' ? 'healthy' : n.status === 'warning' ? 'warning' : 'critical';

    tr.innerHTML = `
      <td>${n.label}</td>
      <td>${n.type}</td>
      <td>${n.region}</td>
      <td><span class="status-badge ${statusClass}">${n.status}</span></td>
      <td class="numeric">${n.cpu.toFixed(2)}</td>
      <td class="numeric">${n.memory.toFixed(1)}</td>
      <td class="numeric">${n.connections}</td>
    `;
    tbody.appendChild(tr);
  });
}

// ---- Sortable headers ----
document.querySelectorAll('.result-table th.sortable').forEach(th => {
  th.addEventListener('click', () => {
    const col = th.dataset.col;
    if (sortCol === col) {
      sortAsc = !sortAsc;
    } else {
      sortCol = col;
      sortAsc = true;
    }
    // Update visual
    document.querySelectorAll('.result-table th').forEach(h => h.classList.remove('sorted'));
    th.classList.add('sorted');
    th.querySelector('.sort-icon').textContent = sortAsc ? '↑' : '↓';
    renderTable();
  });
});

// ---- Properties Panel ----
function renderProperties() {
  const body = document.getElementById('propertiesBody');
  const node = simNodes.find(n => n.id === selectedNodeId);

  if (!node) {
    body.innerHTML = `
      <div style="display:flex;flex-direction:column;align-items:center;justify-content:center;height:100%;color:var(--text-muted);gap:12px;padding:20px;text-align:center;">
        <svg width="40" height="40" viewBox="0 0 40 40" opacity="0.4"><circle cx="14" cy="14" r="5" stroke="var(--accent)" stroke-width="1.2" fill="none"/><circle cx="30" cy="10" r="3" stroke="var(--accent)" stroke-width="1.2" fill="none"/><circle cx="26" cy="30" r="4" stroke="var(--accent)" stroke-width="1.2" fill="none"/><line x1="18" y1="16" x2="28" y2="12" stroke="var(--accent)" stroke-width="0.7" opacity="0.4"/><line x1="16" y1="18" x2="24" y2="27" stroke="var(--accent)" stroke-width="0.7" opacity="0.4"/></svg>
        <div style="font-size:12px;">Select a node to view properties</div>
      </div>`;
    return;
  }

  const color = TYPE_COLORS[node.type] || '#00bcd4';

  // Find edges
  const outgoing = EDGES.filter(e => e.source === node.id);
  const incoming = EDGES.filter(e => e.target === node.id);

  body.innerHTML = `
    <div class="prop-section">
      <div class="prop-node-header">
        <span class="prop-node-icon" style="background:${color}22;border:1.5px solid ${color};box-shadow:0 0 10px ${color}33;"></span>
        <div>
          <div class="prop-node-label">${node.label}</div>
          <div class="prop-node-type" style="color:${color}">${node.type}</div>
        </div>
      </div>
    </div>
    <div class="prop-section">
      <div class="prop-section-title">Attributes</div>
      <table class="prop-table">
        <tr><td class="prop-key">id</td><td class="prop-val">${node.id}</td></tr>
        <tr><td class="prop-key">region</td><td class="prop-val">${node.region}</td></tr>
        <tr><td class="prop-key">status</td><td class="prop-val"><span class="status-badge ${node.status}">${node.status}</span></td></tr>
        <tr><td class="prop-key">cpu_avg</td><td class="prop-val">${node.cpu.toFixed(2)}</td></tr>
        <tr><td class="prop-key">memory_gb</td><td class="prop-val">${node.memory.toFixed(1)}</td></tr>
        <tr><td class="prop-key">connections</td><td class="prop-val">${node.connections}</td></tr>
      </table>
    </div>
    <div class="prop-section">
      <div class="prop-section-title">Connections <span class="prop-count">${outgoing.length + incoming.length}</span></div>
      <div class="prop-edges">
        ${outgoing.map(e => `
          <div class="prop-edge" onclick="selectNode('${e.target}')">
            <span class="edge-direction outgoing">\u2192</span>
            <span class="edge-label">${e.label}</span>
            <span class="edge-target">${NODES.find(n => n.id === e.target)?.label || e.target}</span>
          </div>`).join('')}
        ${incoming.map(e => `
          <div class="prop-edge" onclick="selectNode('${e.source}')">
            <span class="edge-direction incoming">\u2190</span>
            <span class="edge-label">${e.label}</span>
            <span class="edge-target">${NODES.find(n => n.id === e.source)?.label || e.source}</span>
          </div>`).join('')}
      </div>
    </div>`;
}

// ---- Tooltip ----
const tooltip = document.createElement('div');
tooltip.className = 'graph-tooltip';
document.body.appendChild(tooltip);

document.getElementById('graphSvg').addEventListener('mousemove', (ev) => {
  const target = ev.target.closest('.graph-node');
  if (target && !dragNode) {
    const id = target.dataset.id;
    const node = simNodes.find(n => n.id === id);
    if (node) {
      const color = TYPE_COLORS[node.type] || '#00bcd4';
      tooltip.innerHTML = `
        <div><span class="tt-label">${node.label}</span><span class="tt-type">${node.type}</span></div>
        <div class="tt-props">
          <div class="tt-prop"><span class="tt-prop-key">region</span><span class="tt-prop-val">${node.region}</span></div>
          <div class="tt-prop"><span class="tt-prop-key">cpu</span><span class="tt-prop-val">${(node.cpu * 100).toFixed(0)}%</span></div>
          <div class="tt-prop"><span class="tt-prop-key">memory</span><span class="tt-prop-val">${node.memory.toFixed(1)} GB</span></div>
        </div>`;
      tooltip.classList.add('visible');
      tooltip.style.left = (ev.clientX + 12) + 'px';
      tooltip.style.top = (ev.clientY - 10) + 'px';
    }
  } else {
    tooltip.classList.remove('visible');
  }
});
document.getElementById('graphSvg').addEventListener('mouseleave', () => {
  tooltip.classList.remove('visible');
});

// ---- Mini bar chart for R cell output ----
function renderMiniChart() {
  const container = document.getElementById('miniChart');
  if (!container) return;
  const data = NODES.filter(n => n.type === 'Server').slice(0, 8);
  const maxCpu = 1;
  const barWidth = 40;
  const gap = 6;
  const chartWidth = data.length * (barWidth + gap);
  const chartHeight = 100;

  let svg = `<svg width="${chartWidth}" height="${chartHeight + 20}" viewBox="0 0 ${chartWidth} ${chartHeight + 20}" xmlns="http://www.w3.org/2000/svg">`;

  data.forEach((d, i) => {
    const x = i * (barWidth + gap);
    const h = (d.cpu / maxCpu) * chartHeight;
    const y = chartHeight - h;
    const color = d.region === 'us-east-1' ? '#00bcd4' : '#4dd0e1';
    svg += `<rect x="${x}" y="${y}" width="${barWidth}" height="${h}" rx="3" fill="${color}" opacity="0.8"/>`;
    svg += `<text x="${x + barWidth/2}" y="${chartHeight + 14}" text-anchor="middle" font-size="8" fill="#546078" font-family="var(--font-mono)">${d.label.split('-').pop()}</text>`;
  });

  svg += '</svg>';
  container.innerHTML = svg;
}

// ---- Query bar language detection ----
const queryInput = document.getElementById('queryInput');
const langLabel = document.getElementById('langLabel');

const LANG_PATTERNS = [
  { pattern: /^\s*g\./i, lang: 'gremlin' },
  { pattern: /^\s*MATCH\s*\(/i, lang: 'cypher' },
  { pattern: /^\s*(SELECT|CONSTRUCT|ASK|DESCRIBE)\s/i, lang: 'sparql' },
  { pattern: /^\s*(library|require|ggplot|data\.frame|<-)/i, lang: 'r' },
];

queryInput.addEventListener('input', () => {
  const val = queryInput.value;
  for (const { pattern, lang } of LANG_PATTERNS) {
    if (pattern.test(val)) {
      langLabel.textContent = lang;
      return;
    }
  }
});

// Shift+Enter to "run"
queryInput.addEventListener('keydown', (ev) => {
  if (ev.shiftKey && ev.key === 'Enter') {
    ev.preventDefault();
    runQuery();
  }
});

document.getElementById('runBtn').addEventListener('click', runQuery);

function runQuery() {
  const bar = document.getElementById('queryBar');
  bar.style.boxShadow = '0 0 0 2px rgba(0,188,212,0.4)';
  setTimeout(() => { bar.style.boxShadow = ''; }, 400);
}

// ---- Keyboard shortcut: Cmd+P for PDF ----
document.addEventListener('keydown', (ev) => {
  if ((ev.metaKey || ev.ctrlKey) && ev.key === 'p') {
    ev.preventDefault();
    // PDF export stub
  }
});

// ---- Init ----
window.addEventListener('load', () => {
  renderTable();
  renderProperties();
  renderMiniChart();
  initSimulation();
});

window.addEventListener('resize', () => {
  const svg = document.getElementById('graphSvg');
  const rect = svg.parentElement.getBoundingClientRect();
  svgWidth = rect.width || 800;
  svgHeight = rect.height || 500;
});
