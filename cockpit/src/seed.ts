// Demo seed data — 24-node infrastructure topology matching the design mockups.
// Loaded on startup so the cockpit renders fully without a backend.

import type { GraphNode, GraphEdge } from './store';

export const SEED_NODES: GraphNode[] = [
  // Servers
  { id: 'web-server-01', label: 'web-server-01', type: 'Server', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.67, memory_gb: 28.4, connections: 5 } },
  { id: 'web-server-02', label: 'web-server-02', type: 'Server', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.54, memory_gb: 24.1, connections: 4 } },
  { id: 'web-server-03', label: 'web-server-03', type: 'Server', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.42, memory_gb: 31.2, connections: 5 } },
  { id: 'web-server-04', label: 'web-server-04', type: 'Server', properties: { status: 'warning', region: 'eu-west-1', cpu: 0.81, memory_gb: 29.8, connections: 3 } },
  // Gateways
  { id: 'api-gateway-01', label: 'api-gateway-01', type: 'Gateway', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.31, memory_gb: 8.2, connections: 8 } },
  { id: 'api-gateway-02', label: 'api-gateway-02', type: 'Gateway', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.28, memory_gb: 7.9, connections: 7 } },
  // Databases
  { id: 'db-postgres-01', label: 'db-postgres-01', type: 'Database', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.45, memory_gb: 62.3, connections: 6 } },
  { id: 'db-postgres-02', label: 'db-postgres-02', type: 'Database', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.38, memory_gb: 58.7, connections: 5 } },
  // Caches
  { id: 'cache-redis-01', label: 'cache-redis-01', type: 'Cache', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.12, memory_gb: 16.0, connections: 6 } },
  { id: 'cache-redis-02', label: 'cache-redis-02', type: 'Cache', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.09, memory_gb: 16.0, connections: 5 } },
  // Load Balancers
  { id: 'lb-haproxy-01', label: 'lb-haproxy-01', type: 'LoadBalancer', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.22, memory_gb: 4.1, connections: 6 } },
  { id: 'lb-haproxy-02', label: 'lb-haproxy-02', type: 'LoadBalancer', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.19, memory_gb: 3.8, connections: 5 } },
  // Monitoring
  { id: 'prometheus-01', label: 'prometheus-01', type: 'Monitor', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.55, memory_gb: 12.4, connections: 10 } },
  // Message Queue
  { id: 'kafka-broker-01', label: 'kafka-broker-01', type: 'Queue', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.61, memory_gb: 32.0, connections: 8 } },
  // Services
  { id: 'auth-service-01', label: 'auth-service-01', type: 'Service', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.33, memory_gb: 4.2, connections: 6 } },
  { id: 'order-service-01', label: 'order-service-01', type: 'Service', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.47, memory_gb: 6.1, connections: 5 } },
  { id: 'user-service-01', label: 'user-service-01', type: 'Service', properties: { status: 'healthy', region: 'eu-west-1', cpu: 0.29, memory_gb: 3.8, connections: 4 } },
  // Workers
  { id: 'worker-batch-01', label: 'worker-batch-01', type: 'Worker', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.72, memory_gb: 16.4, connections: 3 } },
  // DNS
  { id: 'dns-resolver-01', label: 'dns-resolver-01', type: 'DNS', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.08, memory_gb: 2.1, connections: 12 } },
  // Secrets
  { id: 'vault-01', label: 'vault-01', type: 'Secrets', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.05, memory_gb: 1.8, connections: 8 } },
  // CDN
  { id: 'cdn-edge-01', label: 'cdn-edge-01', type: 'CDN', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.15, memory_gb: 8.0, connections: 4 } },
  // Search
  { id: 'elastic-01', label: 'elastic-01', type: 'Search', properties: { status: 'healthy', region: 'us-east-1', cpu: 0.58, memory_gb: 48.0, connections: 5 } },
  // Extra servers for scale
  { id: 'srv-batch-04', label: 'srv-batch-04', type: 'Server', properties: { status: 'critical', region: 'us-east-1', cpu: 0.95, memory_gb: 30.2, connections: 2 } },
  { id: 'kafka-broker-02', label: 'kafka-broker-02', type: 'Queue', properties: { status: 'warning', region: 'eu-west-1', cpu: 0.78, memory_gb: 31.5, connections: 7 } },
];

export const SEED_EDGES: GraphEdge[] = [
  // LB -> Servers
  { source: 'lb-haproxy-01', target: 'web-server-01', label: 'ROUTES' },
  { source: 'lb-haproxy-01', target: 'web-server-02', label: 'ROUTES' },
  { source: 'lb-haproxy-02', target: 'web-server-03', label: 'ROUTES' },
  { source: 'lb-haproxy-02', target: 'web-server-04', label: 'ROUTES' },
  // Servers -> Gateways
  { source: 'web-server-01', target: 'api-gateway-01', label: 'CALLS' },
  { source: 'web-server-02', target: 'api-gateway-01', label: 'CALLS' },
  { source: 'web-server-03', target: 'api-gateway-02', label: 'CALLS' },
  { source: 'web-server-04', target: 'api-gateway-02', label: 'CALLS' },
  // Gateways -> Services
  { source: 'api-gateway-01', target: 'auth-service-01', label: 'CALLS' },
  { source: 'api-gateway-01', target: 'order-service-01', label: 'CALLS' },
  { source: 'api-gateway-02', target: 'user-service-01', label: 'CALLS' },
  { source: 'api-gateway-02', target: 'auth-service-01', label: 'CALLS' },
  // Services -> Databases
  { source: 'auth-service-01', target: 'db-postgres-01', label: 'READS' },
  { source: 'order-service-01', target: 'db-postgres-01', label: 'WRITES' },
  { source: 'order-service-01', target: 'db-postgres-02', label: 'READS' },
  { source: 'user-service-01', target: 'db-postgres-02', label: 'READS' },
  // Services -> Cache
  { source: 'auth-service-01', target: 'cache-redis-01', label: 'READS' },
  { source: 'order-service-01', target: 'cache-redis-01', label: 'READS' },
  { source: 'user-service-01', target: 'cache-redis-02', label: 'READS' },
  // Cache -> Databases (read-through)
  { source: 'cache-redis-01', target: 'db-postgres-01', label: 'READS_FROM' },
  { source: 'cache-redis-02', target: 'db-postgres-02', label: 'READS_FROM' },
  // Services -> Kafka
  { source: 'order-service-01', target: 'kafka-broker-01', label: 'PUBLISHES' },
  { source: 'auth-service-01', target: 'kafka-broker-01', label: 'PUBLISHES' },
  // Kafka -> Workers
  { source: 'kafka-broker-01', target: 'worker-batch-01', label: 'CONSUMES' },
  { source: 'kafka-broker-02', target: 'worker-batch-01', label: 'CONSUMES' },
  // Workers -> Database
  { source: 'worker-batch-01', target: 'db-postgres-01', label: 'WRITES' },
  // Monitoring
  { source: 'prometheus-01', target: 'web-server-01', label: 'SCRAPES' },
  { source: 'prometheus-01', target: 'api-gateway-01', label: 'SCRAPES' },
  { source: 'prometheus-01', target: 'kafka-broker-01', label: 'SCRAPES' },
  // DNS resolution
  { source: 'dns-resolver-01', target: 'lb-haproxy-01', label: 'RESOLVES' },
  { source: 'dns-resolver-01', target: 'lb-haproxy-02', label: 'RESOLVES' },
  // CDN -> LB
  { source: 'cdn-edge-01', target: 'lb-haproxy-01', label: 'ORIGINS' },
];
