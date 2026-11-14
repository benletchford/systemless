#!/usr/bin/env node
// Read configuration only. Never print complete API responses, environment
// variables, Logpush destinations or traffic records into public CI logs.
const fs = require('node:fs');

const account = process.env.CLOUDFLARE_ACCOUNT_ID;
const token = process.env.CLOUDFLARE_API_TOKEN;
if (!account || !token) throw new Error('Protected Cloudflare credentials are required');
const api = 'https://api.cloudflare.com/client/v4';

async function get(path) {
  try {
    const response = await fetch(api + path, {
      method: 'GET', redirect: 'error', signal: AbortSignal.timeout(15000),
      headers: { Authorization: 'Bearer ' + token },
    });
    const data = await response.json();
    if (!response.ok || !data.success) {
      return { status: 'unavailable', http_status: response.status,
        error_codes: (data.errors || []).map(error => error.code) };
    }
    return { status: 'read', value: data.result, pagination: data.result_info };
  } catch {
    return { status: 'unavailable', reason: 'transport or response error' };
  }
}

function jobs(result) {
  if (result.status !== 'read') return result;
  if (!Array.isArray(result.value)) return { status: 'unknown', reason: 'unexpected response shape' };
  return { status: 'read', returned_job_count: result.value.length,
    pagination_complete: !result.pagination?.total_count || result.value.length >= result.pagination.total_count,
    jobs: result.value.map(job => ({ enabled: job.enabled, dataset: job.dataset,
      fields: (job.output_options?.field_names ||
        (new URLSearchParams(job.logpull_options || '').get('fields') || '').split(','))
        .filter(field => /^[A-Za-z_][A-Za-z0-9_.]*$/.test(field)).sort() })) };
}

(async () => {
  const prefix = '/accounts/' + encodeURIComponent(account);
  const [project, accountJobs, zones] = await Promise.all([
    get(prefix + '/pages/projects/systemless-org'),
    get(prefix + '/logpush/jobs'),
    get('/zones?name=systemless.org&per_page=50'),
  ]);
  const checks = {};
  if (project.status === 'read') {
    const production = project.value.deployment_configs?.production || {};
    const flag = (object, key) => typeof object[key] === 'boolean' ? object[key] : 'not exposed';
    checks.pages = {
      status: 'read', project: project.value.name,
      // Missing response fields are unknown, never evidence of disabled logging.
      observability_enabled: flag(production.observability || {}, 'enabled'),
      logpush_enabled: flag(production, 'logpush'),
      tail_consumer_count: Array.isArray(production.tail_consumers) ? production.tail_consumers.length : 'not exposed',
      analytics_binding_count: production.analytics_engine_datasets
        ? Object.keys(production.analytics_engine_datasets).length : 'not exposed',
      website_analytics_tag_present: Boolean(project.value.web_analytics_tag),
      website_analytics_token_present: Boolean(project.value.web_analytics_token),
    };
  } else checks.pages = project;
  checks.account_logpush = jobs(accountJobs);
  if (zones.status !== 'read') checks.zone = zones;
  else if (!Array.isArray(zones.value) || zones.value.length !== 1) {
    checks.zone = { status: 'unknown', reason: 'expected one readable matching zone',
      matching_count: Array.isArray(zones.value) ? zones.value.length : null };
  } else {
    const zone = '/zones/' + encodeURIComponent(zones.value[0].id);
    checks.zone_logpush = jobs(await get(zone + '/logpush/jobs'));
    checks.zone_instant_logs = jobs(await get(zone + '/logpush/edge/jobs'));
  }
  const report = {
    inspected_at: new Date().toISOString(), source_revision: process.env.GITHUB_SHA || null,
    scope: 'Read-only Pages/Logpush configuration for the native Mac catalogue and asset hosting',
    privacy_declaration_complete: false, checks,
    limitations: [
      'No request records were read. No configuration, retention, permissions or deployment was changed.',
      'Unavailable permissions and absent API fields remain unknown.',
      'This does not establish Cloudflare default security/operational retention or rule out exports outside this token scope.',
      'Website analytics configuration is not evidence that native JSON/asset requests execute a website analytics script.',
      'A successful audit job does not establish a No Data Collected declaration.',
    ],
  };
  fs.writeFileSync('hosting-config-audit.json', JSON.stringify(report, null, 2) + '\n');
  process.stdout.write(JSON.stringify(report, null, 2) + '\n');
})();
