import { at } from './base'
import { isMock } from '.'
import { ApiError, request } from './http'

export type AuthMode = 'local' | 'shared'
export type AuthMethod = 'email' | 'passkey' | 'sso'

export interface AuthMethods {
  mode: AuthMode
  methods: AuthMethod[]
  ssoLabel: string
}

export interface Person {
  user: string
  email: string
  name: string
}

export interface Me {
  mode: AuthMode
  user: Person
  admin: boolean
}

export interface PasskeyInfo {
  id: string
  name: string
  created: number
  used: number | null
}

const ALL: AuthMethod[] = ['email', 'passkey', 'sso']

export const AUTH = {
  methods: '/api/auth/methods',
  me: '/api/auth/me',
  logout: '/api/auth/logout',
  email: '/api/auth/email',
  verify: '/api/auth/email/verify',
  sso: (next: string) => `/api/auth/sso/start?next=${encodeURIComponent(next)}`,
  passkeyOptions: '/api/auth/passkey/options',
  passkey: '/api/auth/passkey',
  passkeys: '/api/account/passkeys',
  enrollOptions: '/api/account/passkeys/options',
  removePasskey: (id: string) => `/api/account/passkeys/${encodeURIComponent(id)}`,
}

const LOCAL: Me = { mode: 'local', user: { user: 'local', email: '', name: '' }, admin: true }

export function isSignedOut(error: unknown): boolean {
  return error instanceof ApiError && error.status === 401
}

export async function authMethods(): Promise<AuthMethods> {
  if (isMock()) return { mode: 'local', methods: ALL, ssoLabel: '' }
  return request<AuthMethods>('GET', AUTH.methods)
}

export async function whoAmI(): Promise<Me | null> {
  if (isMock()) return LOCAL
  try {
    return await request<Me>('GET', AUTH.me)
  } catch (error) {
    if (isSignedOut(error)) return null
    throw error
  }
}

export function sendCode(email: string): Promise<null> {
  return request('POST', AUTH.email, { email })
}

export function verifyCode(email: string, code: string): Promise<Person> {
  return request('POST', AUTH.verify, { email, code })
}

export function logout(): Promise<null> {
  return request('POST', AUTH.logout, {})
}

export function ssoUrl(next: string): string {
  return at(AUTH.sso(next))
}

export function nextPath(search: string): string {
  const next = new URLSearchParams(search).get('next') ?? '/'
  return next.startsWith('/') && !next.startsWith('//') && !next.includes('\\') ? next : '/'
}

function bytes(text: string): ArrayBuffer {
  const base = text.replace(/-/g, '+').replace(/_/g, '/')
  const padded = base + '='.repeat((4 - (base.length % 4)) % 4)
  const raw = atob(padded)
  const out = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) out[i] = raw.charCodeAt(i)
  return out.buffer
}

function text(buffer: ArrayBuffer): string {
  let raw = ''
  for (const byte of new Uint8Array(buffer)) raw += String.fromCharCode(byte)
  return btoa(raw).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

export { bytes as fromBase64Url, text as toBase64Url }

export function passkeysWork(): boolean {
  return typeof window !== 'undefined' && 'PublicKeyCredential' in window && !!navigator.credentials
}

interface SignInOptions {
  challengeId: string
  publicKey: { challenge: string; rpId: string; timeout: number; userVerification: UserVerificationRequirement }
}

export async function signInWithPasskey(): Promise<Person> {
  const options = await request<SignInOptions>('POST', AUTH.passkeyOptions, {})
  const credential = (await navigator.credentials.get({
    publicKey: { ...options.publicKey, challenge: bytes(options.publicKey.challenge) },
  })) as PublicKeyCredential | null
  if (!credential) throw new Error('cancelled')
  const response = credential.response as AuthenticatorAssertionResponse
  return request<Person>('POST', AUTH.passkey, {
    challenge: options.challengeId,
    id: credential.id,
    clientDataJson: text(response.clientDataJSON),
    authenticatorData: text(response.authenticatorData),
    signature: text(response.signature),
  })
}

interface EnrollOptions {
  challengeId: string
  publicKey: {
    challenge: string
    rp: PublicKeyCredentialRpEntity
    user: { id: string; name: string; displayName: string }
    pubKeyCredParams: PublicKeyCredentialParameters[]
    excludeCredentials: { type: 'public-key'; id: string }[]
    authenticatorSelection: AuthenticatorSelectionCriteria
    attestation: AttestationConveyancePreference
    timeout: number
  }
}

export async function addPasskey(name: string): Promise<void> {
  const options = await request<EnrollOptions>('POST', AUTH.enrollOptions, {})
  const key = options.publicKey
  const credential = (await navigator.credentials.create({
    publicKey: {
      ...key,
      challenge: bytes(key.challenge),
      user: { ...key.user, id: bytes(key.user.id) },
      excludeCredentials: key.excludeCredentials.map((c) => ({ type: c.type, id: bytes(c.id) })),
    },
  })) as PublicKeyCredential | null
  if (!credential) throw new Error('cancelled')
  const response = credential.response as AuthenticatorAttestationResponse
  const publicKey = response.getPublicKey()
  if (!publicKey) throw new Error('no public key')
  await request('POST', AUTH.passkeys, {
    challenge: options.challengeId,
    id: credential.id,
    clientDataJson: text(response.clientDataJSON),
    authenticatorData: text(response.getAuthenticatorData()),
    publicKey: text(publicKey),
    publicKeyAlgorithm: response.getPublicKeyAlgorithm(),
    name,
  })
}

export function passkeys(): Promise<PasskeyInfo[]> {
  return request('GET', AUTH.passkeys)
}

export function removePasskey(id: string): Promise<null> {
  return request('DELETE', AUTH.removePasskey(id))
}
