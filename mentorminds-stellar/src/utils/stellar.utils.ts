import { StrKey } from '@stellar/stellar-sdk';

/**
 * Validates a Stellar public key (G...)
 * @param publicKey The public key to validate
 * @returns boolean indicating if the public key is valid
 */
export const isValidPublicKey = (publicKey: string): boolean => {
  try {
    return StrKey.isValidEd25519PublicKey(publicKey);
  } catch {
    return false;
  }
};

/**
 * Shortens a public key for display purposes (e.g., GABC...XYZ)
 * @param publicKey The public key to shorten
 * @param chars Number of characters to show at the start and end
 * @returns Shortened public key string
 */
export const shortenPublicKey = (publicKey: string, chars = 4): string => {
  if (!publicKey) return '';
  if (publicKey.length < chars * 2 + 3) return publicKey;
  return `${publicKey.substring(0, chars)}...${publicKey.substring(publicKey.length - chars)}`;
};

/**
 * Formats a Stellar balance for display
 * @param balance The balance string from Horizon
 * @returns Formatted balance string
 */
export const formatStellarBalance = (balance: string): string => {
  const num = parseFloat(balance);
  return num.toLocaleString(undefined, {
    minimumFractionDigits: 2,
    maximumFractionDigits: 7,
  });
};
