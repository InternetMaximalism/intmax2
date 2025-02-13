package utils

import (
	"math/big"

	"github.com/consensys/gnark/backend/witness"
)

func ExtractPublicInputs(witness witness.Witness) ([]*big.Int, error) {
    public, err := witness.Public()
    if err != nil {
        return nil, err
    }
    _publicBytes, _ := public.MarshalBinary()
    publicBytes := _publicBytes[12:] 
    const chunkSize = 32 
    bigInts := make([]*big.Int, len(publicBytes)/chunkSize)
    for i := 0; i < len(publicBytes)/chunkSize; i += 1 {
        chunk := publicBytes[i*chunkSize : (i+1)*chunkSize]
        bigInt := new(big.Int).SetBytes(chunk)
        bigInts[i] = bigInt
    }
    return bigInts, nil
}
