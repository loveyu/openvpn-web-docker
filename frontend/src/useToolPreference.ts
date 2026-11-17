import { useState } from "react";

export function useToolPreference<T extends string | number>(
  key: string,
  defaultValue: T,
  choices?: readonly T[],
) {
  const storageKey = `vpnweb-tool-${key}`;
  const [value, setValue] = useState<T>(() => {
    try {
      const stored: unknown = JSON.parse(localStorage.getItem(storageKey) ?? "null");
      if (typeof stored === typeof defaultValue && (!choices || choices.includes(stored as T))) {
        return stored as T;
      }
    } catch {
      // 缓存损坏或本地存储不可用时，使用原有默认值。
    }
    return defaultValue;
  });

  function updateValue(next: T) {
    setValue(next);
    try {
      localStorage.setItem(storageKey, JSON.stringify(next));
    } catch {
      // 本地存储不可用时，仍允许在当前表单中编辑和使用参数。
    }
  }

  return [value, updateValue] as const;
}
