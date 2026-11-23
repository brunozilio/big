export interface Identity {
  user: string;
  account: string | null;
  role: string | null;
  expires: string | null;
}

export interface AccountRoles {
  account: string;
  name: string;
  roles: string[];
}

export interface Cloud {
  identity: { args: Record<string, never>; result: Identity };
  accounts_and_roles: { args: Record<string, never>; result: AccountRoles[] };
}
