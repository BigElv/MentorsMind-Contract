import { detectFreighter, validateStellarKey } from './wallet.service';

describe('Wallet Service', () => {
  it('detectFreighter returns false when no extension is installed', () => {
    (global as any).window = undefined;
    expect(detectFreighter()).toBe(false);
  });

  it('validateStellarKey accepts a valid public key', () => {
    expect(validateStellarKey('GBRPYHIL2C7F7Y5Q6QNU3QC6YIVP7XCHV6A6I3BKU53Z3O6YUBOWDDFP')).toBe(true);
  });

  it('validateStellarKey rejects bad key', () => {
    expect(validateStellarKey('not-a-valid-key')).toBe(false);
  });
});
