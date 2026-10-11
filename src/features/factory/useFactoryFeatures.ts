import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { factoryApi } from './api';
import type { Feature } from './types';
export type FactoryAction = (action: () => Promise<unknown>, success: string) => Promise<void>;
export function useFactoryFeatures() {
  const [features, setFeatures] = useState<Feature[]>([]);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const mounted = useRef(true);
  const refreshing = useRef(false);
  const refresh = useCallback(async () => {
    if (refreshing.current) return;
    refreshing.current = true;
    try {
      const result = await factoryApi.list();
      if (mounted.current) setFeatures(result);
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      refreshing.current = false;
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void refresh();
    const timer = window.setInterval(() => void refresh(), 2000);
    const subscription = listen<string>('factory-progress', () => void refresh());
    return () => {
      mounted.current = false;
      window.clearInterval(timer);
      void subscription.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [refresh]);

  const act = async (action: () => Promise<unknown>, success: string) => {
    setPending(true);
    setError('');
    setNotice('');
    try {
      await action();
      if (mounted.current) {
        setNotice(success);
        await refresh();
      }
    } catch (cause) {
      if (mounted.current) setError(String(cause));
    } finally {
      if (mounted.current) setPending(false);
    }
  };

  return { features, pending, error, notice, act };
}
