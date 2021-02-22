import { Connection, PublicKey } from '@solana/web3.js';
import { OptionMarket, OptionWriter } from '../market';
import { getOptionMarketData } from './getOptionMarketData';

/**
 * Returns a tuple containing a random option writer and the Option Market data
 *
 * @param connection solana web3 connection
 * @param optionMarketKey Pubkey of the Option Market data account
 */
export const getRandomOptionWriter = async (
  connection: Connection,
  optionMarketKey: PublicKey,
): Promise<[OptionWriter, OptionMarket]> => {
  const optionMarketData = await getOptionMarketData(
    connection,
    optionMarketKey,
  );
  const randRegistryIndex = Math.floor(
    Math.random() * (optionMarketData.registryLength - 1),
  );

  return [
    optionMarketData.optionWriterRegistry[randRegistryIndex],
    optionMarketData,
  ];
};