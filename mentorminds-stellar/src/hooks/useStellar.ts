import { useStellar as useStellarContext } from '../contexts/StellarContext';

/**
 * Custom hook for using Stellar functionality throughout the app
 */
export const useStellar = () => {
  return useStellarContext();
};
