// Types for sw/push.js (imported by its Vitest test).

export interface Shown {
  delivery_id: string;
  notice_id: string;
  reminder_id: string | null;
  occurrence: string | null;
  title: string;
  body: string;
  url: string;
  tag: string;
}

export interface NotificationData {
  delivery_id?: string;
  reminder_id?: string | null;
  occurrence?: string | null;
  url?: string;
}

export interface Built {
  title: string;
  options: {
    body: string;
    tag: string;
    renotify?: boolean;
    requireInteraction?: boolean;
    icon: string;
    badge?: string;
    data: NotificationData;
    actions?: { action: string; title: string }[];
  };
}

export const NOTIFICATION_SNOOZE_MINUTES: number;
export function notificationFor(shown: Shown): Built;
export function genericNotification(): Built;
export function actionRequest(
  action: string,
  data: NotificationData | null | undefined,
): { path: string; body: Record<string, unknown>; ack: string } | null;
export function openPath(data: NotificationData | null | undefined): string;
