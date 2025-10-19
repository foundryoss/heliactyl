import { Hono } from 'hono';
import type { HeliaModule, ModuleRegisterContext } from '../../../types/index';
import type { Context } from 'hono';
import type { AppEnv } from '../../lib/hono-env';
import mqttConfigRouter from './mqtt-config';

const manifest = {
  name: 'Core',
  author: 'Heliactyl',
  targetVersion: '15.0.0',
  description: 'Core health and info routes',
} as const;

const router = new Hono<AppEnv>();

router.get('/health', (c: Context<AppEnv>) => c.json({ ok: true }));

router.get('/info', (c: Context<AppEnv>) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'] ?? {};
  return c.json({
    name: cfg.app?.name,
    version: cfg.app?.version,
  });
});

router.get('/locations', (c: Context<AppEnv>) => {
  const cfg = c.get('config') as AppEnv['Variables']['config'];
  const locations = cfg.locations || [];
  return c.json({ items: locations });
});

router.get('/eggs', async (c: Context<AppEnv>) => {
  const { client } = c.get('db') as AppEnv['Variables']['db'];
  const rows = await client`SELECT eggId, name, dockerImage FROM eggs ORDER BY nestId, eggId`;
  return c.json({ items: rows });
});

// Mount MQTT config routes
router.route('/mqtt', mqttConfigRouter);

const mod: HeliaModule = {
  manifest,
  register: ({ app }: ModuleRegisterContext) => {
    (app as any).route('/core', router);
  },
};

export default mod;


