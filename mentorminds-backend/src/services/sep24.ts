import axios from 'axios';

export interface SEP24Transaction {
  id: string;
  kind: 'deposit' | 'withdrawal';
  status: string;
  more_info_url?: string;
  amount_in?: string;
  amount_out?: string;
  amount_fee?: string;
  stellar_transaction_id?: string;
  message?: string;
}

export class SEP24Service {
  /**
   * Start an interactive deposit flow.
   */
  static async initiateDeposit(
    transferServerUrl: string,
    authToken: string,
    assetCode: string,
    account: string,
    amount?: string
  ): Promise<{ url: string; id: string }> {
    const response = await axios.post(
      `${transferServerUrl}/transactions/deposit/interactive`,
      {
        asset_code: assetCode,
        account: account,
        amount: amount
      },
      {
        headers: { Authorization: `Bearer ${authToken}` }
      }
    );

    return {
      url: response.data.url,
      id: response.data.id
    };
  }

  /**
   * Start an interactive withdrawal flow.
   */
  static async initiateWithdrawal(
    transferServerUrl: string,
    authToken: string,
    assetCode: string,
    account: string,
    amount?: string
  ): Promise<{ url: string; id: string }> {
    const response = await axios.post(
      `${transferServerUrl}/transactions/withdraw/interactive`,
      {
        asset_code: assetCode,
        account: account,
        amount: amount
      },
      {
        headers: { Authorization: `Bearer ${authToken}` }
      }
    );

    return {
      url: response.data.url,
      id: response.data.id
    };
  }

  /**
   * Poll transaction status until it changes or hits a timeout.
   */
  static async getTransactionStatus(
    transferServerUrl: string,
    authToken: string,
    transactionId: string
  ): Promise<SEP24Transaction> {
    const response = await axios.get(
      `${transferServerUrl}/transactions/${transactionId}`,
      {
        headers: { Authorization: `Bearer ${authToken}` }
      }
    );
    return response.data.transaction;
  }
}
