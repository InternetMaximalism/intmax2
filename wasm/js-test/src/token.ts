export enum TokenType {
  Native = 0,
  ERC20 = 1,
  ERC721 = 2,
  ERC1155 = 3
}

export const TokenTypeNames: Record<number, string> = {
  [TokenType.Native]: "Native",
  [TokenType.ERC20]: "ERC20",
  [TokenType.ERC721]: "ERC721",
  [TokenType.ERC1155]: "ERC1155"
};
