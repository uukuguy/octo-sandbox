import { api } from './index';
import { LoginRequest, LoginResponse, RegisterRequest, RegisterResponse } from './types';

export const authApi = {
  async login(req: LoginRequest): Promise<LoginResponse> {
    const data = await api.request<LoginResponse>('/auth/login', {
      method: 'POST',
      body: JSON.stringify(req),
    });
    api.setToken(data.access_token);
    api.setRefreshToken(data.refresh_token);
    return data;
  },

  async register(req: RegisterRequest): Promise<RegisterResponse> {
    return api.request<RegisterResponse>('/auth/register', {
      method: 'POST',
      body: JSON.stringify(req),
    });
  },

  async refresh(refreshToken: string): Promise<LoginResponse> {
    const data = await api.request<LoginResponse>('/auth/refresh', {
      method: 'POST',
      body: JSON.stringify({ refresh_token: refreshToken }),
    });
    api.setToken(data.access_token);
    api.setRefreshToken(data.refresh_token);
    return data;
  },

  logout() {
    api.clearTokens();
  },
};
