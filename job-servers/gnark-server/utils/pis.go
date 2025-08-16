package utils

import (
	"fmt"
	"math/big"

	"github.com/consensys/gnark/backend/witness"
)

func CalculateInputDigest(publicInputs []uint64) (*big.Int, error) {
	if len(publicInputs) != 8 {
		return nil, fmt.Errorf("expected 8 public inputs, got %d", len(publicInputs))
	}

	// Validate first element is within 29 bits
	if publicInputs[0] > (1<<29 - 1) {
		return nil, fmt.Errorf("first public input exceeds 29 bits: %d (max: %d)",
			publicInputs[0], 1<<29-1)
	}

	// Validate remaining elements are within 32 bits
	for i := 1; i < 8; i++ {
		if publicInputs[i] > (1<<32 - 1) {
			return nil, fmt.Errorf("public input[%d] exceeds 32 bits: %d (max: %d)",
				i, publicInputs[i], 1<<32-1)
		}
	}

	inputDigest := big.NewInt(0)
	for i := 0; i < 8; i++ {
		value := new(big.Int).SetUint64(publicInputs[7-i])
		bitPosition := uint(32 * i)
		value.Lsh(value, bitPosition)
		inputDigest.Add(inputDigest, value)
	}

	return inputDigest, nil
}

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
