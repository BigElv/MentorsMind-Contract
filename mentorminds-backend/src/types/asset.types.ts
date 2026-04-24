/**
 * Asset type definitions for multi-currency support
 * Defines types for assets, exchange rates, trustlines, balances, and fees
 * on the Stellar network.
 */

/**
 * Union type representing supported asset codes on the Stellar network.
 * - XLM: Stellar Lumens (native asset)
 * - USDC: USD Coin (stablecoin)
 * - PYUSD: PayPal USD (stablecoin)
 */
export type AssetCode = 'XLM' | 'USDC' | 'PYUSD';

/**
 * Represents a digital asset on the Stellar network.
 * Contains metadata about the asset including its code, issuer, precision, and display name.
 *
 * @property code - The three-letter asset code (XLM, USDC, PYUSD)
 * @property issuer - The account address that issued the asset, or null for native asset (XLM)
 * @property decimals - The number of decimal places for precision (7 for XLM, 6 for USDC/PYUSD)
 * @property name - Human-readable name of the asset (e.g., "Stellar Lumens", "USD Coin")
 */
export interface Asset {
  code: AssetCode;
  issuer: string | null;
  decimals: number;
  name: string;
}

/**
 * Represents an exchange rate between two assets.
 * Stores the conversion ratio and timestamp for caching purposes.
 *
 * @property fromAsset - The source asset code
 * @property toAsset - The destination asset code
 * @property rate - The conversion rate (e.g., 0.0875 means 1 fromAsset = 0.0875 toAsset)
 * @property timestamp - Unix timestamp when the rate was fetched
 */
export interface ExchangeRate {
  fromAsset: AssetCode;
  toAsset: AssetCode;
  rate: number;
  timestamp: number;
}

/**
 * Represents a trustline for a non-native asset on an account.
 * A trustline allows an account to hold and transact with a specific asset.
 *
 * @property asset - The asset code for this trustline
 * @property balance - The current balance as a string (e.g., "100.000000")
 * @property limit - The maximum amount that can be held as a string
 * @property authorized - Whether the trustline is authorized by the issuer
 */
export interface Trustline {
  asset: AssetCode;
  balance: string;
  limit: string;
  authorized: boolean;
}

/**
 * Represents the balance of an asset in an account.
 * Includes both the raw amount and a formatted display version.
 *
 * @property asset - The asset code
 * @property amount - The raw balance amount as a string
 * @property formattedAmount - The balance formatted for display with proper decimal places
 */
export interface AssetBalance {
  asset: AssetCode;
  amount: string;
  formattedAmount: string;
}

/**
 * Represents a path payment transaction on the Stellar network.
 * Path payments convert one asset to another through a series of trades.
 *
 * @property sendAsset - The asset being sent
 * @property sendAmount - The amount of sendAsset to send as a string
 * @property path - Array of intermediate assets in the conversion path (e.g., ['XLM'] for USDC → XLM → PYUSD)
 * @property receiveAsset - The asset being received
 * @property receiveAmount - The amount of receiveAsset expected to receive as a string
 */
export interface PathPayment {
  sendAsset: AssetCode;
  sendAmount: string;
  path: AssetCode[];
  receiveAsset: AssetCode;
  receiveAmount: string;
}

/**
 * Represents the fee breakdown for a transaction in a specific asset.
 * Includes base fees and additional fees for path payments.
 *
 * @property baseFee - The base transaction fee as a string (100 stroops = 0.00001 XLM)
 * @property pathFee - Additional fee for path complexity as a string (0 for standard payments)
 * @property totalFee - The total fee (baseFee + pathFee) as a string
 * @property feeAsset - The asset in which fees are calculated
 */
export interface AssetFees {
  baseFee: string;
  pathFee: string;
  totalFee: string;
  feeAsset: AssetCode;
}
