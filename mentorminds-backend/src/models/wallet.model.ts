import { Pool } from "pg";
import { CacheService } from "./cache.service";

export interface WalletRecord {
  id: string;
  userId: string;
  stellarPublicKey: string;
  createdAt: Date;
  updatedAt: Date;
}

const CACHE_TTL_MS = 45_000; // 45 seconds

function stellarKeyCache(publicKey: string): string {
  return `mm:wallet:by-stellar-key:${publicKey}`;
}

function userIdCacheKey(userId: string): string {
  return `mm:wallet:by-user-id:${userId}`;
}

export class WalletModel {
  constructor(
    private readonly pool: Pool,
    private readonly cache: CacheService
  ) {}

  async findByStellarPublicKey(publicKey: string): Promise<WalletRecord | null> {
    const cacheKey = stellarKeyCache(publicKey);
    const cached = this.cache.get<WalletRecord | null>(cacheKey);
    if (cached !== null) return cached;

    const result = await this.pool.query<WalletRecord>(
      "SELECT id, user_id AS \"userId\", stellar_public_key AS \"stellarPublicKey\", created_at AS \"createdAt\", updated_at AS \"updatedAt\" FROM wallets WHERE stellar_public_key = $1 LIMIT 1",
      [publicKey]
    );

    const wallet = result.rows[0] ?? null;
    this.cache.set(cacheKey, wallet, CACHE_TTL_MS);
    return wallet;
  }

  async findByUserId(userId: string): Promise<WalletRecord | null> {
    const cacheKey = userIdCacheKey(userId);
    const cached = this.cache.get<WalletRecord | null>(cacheKey);
    if (cached !== null) return cached;

    const result = await this.pool.query<WalletRecord>(
      "SELECT id, user_id AS \"userId\", stellar_public_key AS \"stellarPublicKey\", created_at AS \"createdAt\", updated_at AS \"updatedAt\" FROM wallets WHERE user_id = $1 LIMIT 1",
      [userId]
    );

    const wallet = result.rows[0] ?? null;
    this.cache.set(cacheKey, wallet, CACHE_TTL_MS);
    return wallet;
  }

  /**
   * Update a wallet's stellar_public_key and invalidate related cache entries.
   */
  async updateStellarPublicKey(
    walletId: string,
    newPublicKey: string
  ): Promise<WalletRecord | null> {
    const result = await this.pool.query<WalletRecord>(
      "UPDATE wallets SET stellar_public_key = $1, updated_at = NOW() WHERE id = $2 RETURNING id, user_id AS \"userId\", stellar_public_key AS \"stellarPublicKey\", created_at AS \"createdAt\", updated_at AS \"updatedAt\"",
      [newPublicKey, walletId]
    );

    const updated = result.rows[0] ?? null;
    if (updated) {
      this.cache.del(stellarKeyCache(newPublicKey));
      this.cache.del(userIdCacheKey(updated.userId));
    }

    return updated;
  }
}
