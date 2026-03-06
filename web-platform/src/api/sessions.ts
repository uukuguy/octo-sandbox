import { api } from './index';
import { Session, CreateSessionRequest } from './types';

export const sessionsApi = {
  async list(): Promise<Session[]> {
    return api.request<Session[]>('/sessions', { method: 'GET' });
  },

  async create(req?: CreateSessionRequest): Promise<Session> {
    return api.request<Session>('/sessions', {
      method: 'POST',
      body: JSON.stringify(req || {}),
    });
  },

  async get(sessionId: string): Promise<Session> {
    return api.request<Session>(`/sessions/${sessionId}`, { method: 'GET' });
  },

  async delete(sessionId: string): Promise<void> {
    await api.request(`/sessions/${sessionId}`, { method: 'DELETE' });
  },
};
