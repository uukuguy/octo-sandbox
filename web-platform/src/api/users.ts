import { api } from './index';
import { User } from './types';

export const usersApi = {
  async me(): Promise<User> {
    return api.request<User>('/auth/me', { method: 'GET' });
  },

  async list(): Promise<User[]> {
    return api.request<User[]>('/users', { method: 'GET' });
  },

  async get(userId: string): Promise<User> {
    return api.request<User>(`/users/${userId}`, { method: 'GET' });
  },

  async update(userId: string, data: Partial<User>): Promise<User> {
    return api.request<User>(`/users/${userId}`, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },

  async updateRole(userId: string, role: string): Promise<User> {
    return api.request<User>(`/users/${userId}/role`, {
      method: 'PATCH',
      body: JSON.stringify({ role }),
    });
  },
};
