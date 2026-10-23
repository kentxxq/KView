import type { ComputedRef, InjectionKey } from "vue";

export const logModeControlKey: InjectionKey<{
  visible: ComputedRef<boolean>;
  enabled: ComputedRef<boolean>;
  disabled: ComputedRef<boolean>;
  toggle: (enabled: boolean) => Promise<void>;
}> = Symbol("log-mode-control");
