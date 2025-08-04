package main

import (
	"fmt"
	"os"

	verifierCircuit "gnark-server/circuit"
	"gnark-server/trusted_setup"
	"gnark-server/utils"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark-crypto/kzg"
	"github.com/consensys/gnark/backend/plonk"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/scs"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
)

func loadCircuit() constraint.ConstraintSystem {
	commonCircuitData := types.ReadCommonCircuitData("data/common_circuit_data.json")
	proofRaw := types.ReadProofWithPublicInputs("data/proof_with_public_inputs.json")
	proofWithPis := variables.DeserializeProofWithPublicInputs(proofRaw)
	verifierOnlyCircuitData := variables.DeserializeVerifierOnlyCircuitData(types.ReadVerifierOnlyCircuitData("data/verifier_only_circuit_data.json"))
	inputHash, err := utils.CalculateInputDigest(proofRaw.PublicInputs)
	if err != nil {
		panic(fmt.Sprintf("failed to calculate input digest: %v", err))
	}
	circuit := verifierCircuit.VerifierCircuit{
		VerifierDigest:    verifierOnlyCircuitData.CircuitDigest,
		InputHash:         inputHash,
		VerifierData:      verifierOnlyCircuitData,
		ProofWithPis:      proofWithPis,
		CommonCircuitData: commonCircuitData,
	}
	builder := scs.NewBuilder
	ccs, err := frontend.Compile(ecc.BN254.ScalarField(), builder, &circuit)
	if err != nil {
		panic(err)
	}
	return ccs
}

func main() {
	r1cs := loadCircuit()

	proofRaw := types.ReadProofWithPublicInputs("data/proof_with_public_inputs.json")
	proofWithPis := variables.DeserializeProofWithPublicInputs(proofRaw)
	verifierOnlyCircuitData := variables.DeserializeVerifierOnlyCircuitData(types.ReadVerifierOnlyCircuitData("data/verifier_only_circuit_data.json"))
	inputHash, err := utils.CalculateInputDigest(proofRaw.PublicInputs)
	if err != nil {
		panic(fmt.Sprintf("failed to calculate input digest: %v", err))
	}

	// 1. One setup
	var srs kzg.SRS = kzg.NewSRS(ecc.BN254)
	{
		fileName := "srs_setup"

		if _, err := os.Stat(fileName); os.IsNotExist(err) {
			trusted_setup.DownloadAndSaveAztecIgnitionSrs(174, fileName)
		}

		fSRS, err := os.Open(fileName)

		if err != nil {
			panic(err)
		}

		_, err = srs.ReadFrom(fSRS)

		fSRS.Close()

		if err != nil {
			panic(err)
		}
	}
	pk, vk, err := plonk.Setup(r1cs, srs)
	if err != nil {
		fmt.Println(err)
		os.Exit(1)
	}
	assignment := verifierCircuit.VerifierCircuit{
		VerifierDigest:    verifierOnlyCircuitData.CircuitDigest,
		InputHash:         inputHash,
		ProofWithPis:      proofWithPis,
		VerifierData:      verifierOnlyCircuitData,
		CommonCircuitData: types.ReadCommonCircuitData("data/common_circuit_data.json"),
	}
	witness, err := frontend.NewWitness(&assignment, ecc.BN254.ScalarField())
	if err != nil {
		panic(err)
	}
	proof, err := plonk.Prove(r1cs, pk, witness)
	if err != nil {
		panic(err)
	}
	// 3. Proof verification
	witnessPublic, err := witness.Public()
	if err != nil {
		panic(err)
	}
	err = plonk.Verify(proof, vk, witnessPublic)
	if err != nil {
		panic(err)
	}
	{
		fSol, _ := os.Create("data/verifier.sol")
		_ = vk.ExportSolidity(fSol)
		fSol.Close()
	}
	{
		fVk, _ := os.Create("data/verifying.key")
		_, _ = vk.WriteTo(fVk)
		fVk.Close()
	}
	{
		fPk, _ := os.Create("data/proving.key")
		_, _ = pk.WriteTo(fPk)
		fPk.Close()
	}
	{
		fCs, _ := os.Create("data/circuit.r1cs")
		_, _ = r1cs.WriteTo(fCs)
		fCs.Close()
	}
	fmt.Println("Setup done!")
}
