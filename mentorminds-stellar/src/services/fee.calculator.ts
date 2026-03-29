import type { Horizon } from '@stellar/stellar-sdk';

import type { FeeDetails } from '../types/transaction.types';

const STROOPS_PER_XLM = 10_000_000n;

function stroopsToXlm(stroops: bigint): string {
  const whole = stroops / STROOPS_PER_XLM;
  const fractional = (stroops % STROOPS_PER_XLM).toString().padStart(7, '0').replace(/0+$/, '');
  return fractional ? `${whole.toString()}.${fractional}` : whole.toString();
}

export class FeeCalculator {
  public static async getCurrentBaseFee(server: Horizon.Server): Promise<bigint> {
    const baseFee = await server.fetchBaseFee();
    return BigInt(baseFee);
  }

  public static async calculateSuggestedFee(
    server: Horizon.Server,
    operationCount: number,
  ): Promise<FeeDetails> {
    const baseFee = await FeeCalculator.getCurrentBaseFee(server);
    const suggestedFee = baseFee * BigInt(Math.max(operationCount, 1));

    return {
      baseFeeStroops: baseFee.toString(),
      suggestedFeeStroops: suggestedFee.toString(),
      suggestedFeeXlm: stroopsToXlm(suggestedFee),
      operationCount: Math.max(operationCount, 1),
    };
  }
}
