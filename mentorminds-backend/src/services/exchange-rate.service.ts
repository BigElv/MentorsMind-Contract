/**
 * Exchange Rate Service
 * Fetches and caches exchange rates from the Stellar DEX via Horizon API.
 * Provides methods to query rates for asset pairs with automatic caching
 * and TTL-based invalidation.
 */

import { AssetCode } from '../types/asset.types';

/**
 * Represents a cached exchange rate entry with timestamp and TTL.
 * Used internally to track cache validity.
 *
 * @property rate - The exchange rate as a decimal number
 * @property timestamp - Unix timestamp (milliseconds) when the rate was fetched
 * @property ttl - Time-to-live in milliseconds (60 seconds = 60000ms)
 */
interface CacheEntry {
  rate: number;
  timestamp: number;
  ttl: number;
}

/**
 * Represents the status of the exchange rate cache.
 * Used for monitoring and debugging cache state.
 *
 * @property entries - Total number of cached entries
 * @property rates - Array of cached rate information with expiration times
 */
interface CacheStatus {
  entries: number;
  rates: Array<{
    pair: string;
    rate: number;
    expiresIn: number; // seconds remaining
  }>;
}

/**
 * Asset issuer addresses on the Stellar network.
 * Used to construct order book queries for non-native assets.
 */
const ASSET_ISSUERS: Record<Exclude<AssetCode, 'XLM'>, string> = {
  USDC: 'GBBD47UZQ2BNSE7E2CMML7BNPI5BEFF2KE5FIXEDISSUERADDRESS',
  PYUSD: 'GDZ55LVXECRTW4G36ICJVWCIHL7BQUM2FixedIssuerAddress',
};

/**
 * ExchangeRateService class
 * Manages fetching and caching of exchange rates from the Stellar DEX.
 * Implements 60-second TTL caching to reduce API calls.
 */
class ExchangeRateService {
  private cache: Map<string, CacheEntry> = new Map();
  private horizonUrl: string;

  /**
   * Initialize the ExchangeRateService with a Horizon API endpoint.
   * @param horizonUrl - The Horizon API base URL (default: Stellar public network)
   */
  constructor(horizonUrl: string = 'https://horizon.stellar.org') {
    this.horizonUrl = horizonUrl;
  }

  /**
   * Generate a cache key for an asset pair.
   * @param fromAsset - The source asset code
   * @param toAsset - The destination asset code
   * @returns A string key in format "FROM/TO"
   */
  private getCacheKey(fromAsset: AssetCode, toAsset: AssetCode): string {
    return `${fromAsset}/${toAsset}`;
  }

  /**
   * Check if a cache entry is still valid (within TTL).
   * @param entry - The cache entry to validate
   * @returns true if the entry is within its TTL, false if expired
   */
  private isCacheValid(entry: CacheEntry): boolean {
    const now = Date.now();
    const age = now - entry.timestamp;
    return age < entry.ttl;
  }

  /**
   * Query the Stellar Horizon API for an order book to determine exchange rate.
   * Fetches the best bid/ask prices and calculates an effective rate.
   *
   * @param fromAsset - The source asset code
   * @param toAsset - The destination asset code
   * @returns The exchange rate as a decimal number
   * @throws Error if the API request fails or no trading path exists
   */
  private async queryHorizonAPI(
    fromAsset: AssetCode,
    toAsset: AssetCode
  ): Promise<number> {
    try {
      // Build order book query parameters
      const params = new URLSearchParams();

      // Set selling asset (fromAsset)
      if (fromAsset === 'XLM') {
        params.append('selling_asset_type', 'native');
      } else {
        params.append('selling_asset_type', 'credit_alphanum12');
        params.append('selling_asset_code', fromAsset);
        params.append('selling_asset_issuer', ASSET_ISSUERS[fromAsset as Exclude<AssetCode, 'XLM'>]);
      }

      // Set buying asset (toAsset)
      if (toAsset === 'XLM') {
        params.append('buying_asset_type', 'native');
      } else {
        params.append('buying_asset_type', 'credit_alphanum12');
        params.append('buying_asset_code', toAsset);
        params.append('buying_asset_issuer', ASSET_ISSUERS[toAsset as Exclude<AssetCode, 'XLM'>]);
      }

      const url = `${this.horizonUrl}/order_book?${params.toString()}`;
      const response = await fetch(url);

      if (!response.ok) {
        throw new Error(
          `Horizon API error: ${response.status} ${response.statusText}`
        );
      }

      const data = await response.json();

      // Check if there are any asks (sellers willing to sell toAsset for fromAsset)
      if (!data.asks || data.asks.length === 0) {
        throw new Error(
          `No trading path available for ${fromAsset}/${toAsset}`
        );
      }

      // Use the best ask price (lowest price at which someone will sell)
      // The price is in terms of: 1 unit of selling_asset = price units of buying_asset
      const bestAsk = data.asks[0];
      const rate = parseFloat(bestAsk.price);

      if (isNaN(rate) || rate <= 0) {
        throw new Error(
          `Invalid exchange rate received: ${bestAsk.price}`
        );
      }

      return rate;
    } catch (error) {
      if (error instanceof Error) {
        throw error;
      }
      throw new Error(`Failed to fetch exchange rate: ${String(error)}`);
    }
  }

  /**
   * Fetch the exchange rate for an asset pair with caching.
   * Returns cached rate if available and valid, otherwise queries Horizon API.
   *
   * @param fromAsset - The source asset code
   * @param toAsset - The destination asset code
   * @returns The exchange rate as a decimal number (e.g., 0.0875 for 1 XLM = 0.0875 USDC)
   * @throws Error if the API request fails or no trading path exists
   */
  async fetchExchangeRate(
    fromAsset: AssetCode,
    toAsset: AssetCode
  ): Promise<number> {
    // Same asset always has rate of 1
    if (fromAsset === toAsset) {
      return 1;
    }

    const cacheKey = this.getCacheKey(fromAsset, toAsset);
    const cached = this.cache.get(cacheKey);

    // Return cached rate if valid
    if (cached && this.isCacheValid(cached)) {
      return cached.rate;
    }

    // Fetch fresh rate from API
    const rate = await this.queryHorizonAPI(fromAsset, toAsset);

    // Store in cache with 60-second TTL
    this.cache.set(cacheKey, {
      rate,
      timestamp: Date.now(),
      ttl: 60000, // 60 seconds
    });

    return rate;
  }

  /**
   * Invalidate the cache entry for a specific asset pair.
   * Forces the next fetch to query the API.
   *
   * @param fromAsset - The source asset code
   * @param toAsset - The destination asset code
   */
  invalidateCache(fromAsset: AssetCode, toAsset: AssetCode): void {
    const cacheKey = this.getCacheKey(fromAsset, toAsset);
    this.cache.delete(cacheKey);
  }

  /**
   * Get the current status of the exchange rate cache.
   * Useful for monitoring and debugging cache state.
   *
   * @returns CacheStatus object with entry count and rate information
   */
  getCacheStatus(): CacheStatus {
    const rates: CacheStatus['rates'] = [];
    const now = Date.now();

    for (const [pair, entry] of this.cache.entries()) {
      const expiresIn = Math.max(
        0,
        Math.ceil((entry.ttl - (now - entry.timestamp)) / 1000)
      );
      rates.push({
        pair,
        rate: entry.rate,
        expiresIn,
      });
    }

    return {
      entries: this.cache.size,
      rates,
    };
  }

  /**
   * Fetch multiple exchange rates in parallel.
   * More efficient than calling fetchExchangeRate multiple times.
   *
   * @param pairs - Array of [fromAsset, toAsset] tuples to fetch
   * @returns Map with cache keys as keys and rates as values
   * @throws Error if any API request fails
   */
  async fetchMultipleRates(
    pairs: Array<[AssetCode, AssetCode]>
  ): Promise<Map<string, number>> {
    const promises = pairs.map(([from, to]) =>
      this.fetchExchangeRate(from, to).then((rate) => ({
        key: this.getCacheKey(from, to),
        rate,
      }))
    );

    const results = await Promise.all(promises);
    const ratesMap = new Map<string, number>();

    for (const { key, rate } of results) {
      ratesMap.set(key, rate);
    }

    return ratesMap;
  }
}

/**
 * Singleton instance of ExchangeRateService for use across the application.
 * Ensures consistent caching and API usage throughout the app.
 */
const exchangeRateService = new ExchangeRateService();

export { ExchangeRateService, exchangeRateService, CacheEntry, CacheStatus };
