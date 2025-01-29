package main

import (
	"flag"
	"fmt"
	"os"

	verifierCircuit "gnark-server/circuit"
	"gnark-server/trusted_setup"

	"github.com/consensys/gnark-crypto/ecc"
	"github.com/consensys/gnark-crypto/kzg"
	"github.com/consensys/gnark/backend/plonk"
	"github.com/consensys/gnark/constraint"
	"github.com/consensys/gnark/frontend"
	"github.com/consensys/gnark/frontend/cs/scs"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
)

func loadCircuit(circuitName string) constraint.ConstraintSystem {
	commonCircuitData := types.ReadCommonCircuitData("data/" + circuitName + "/common_circuit_data.json")
	proofWithPis := variables.DeserializeProofWithPublicInputs(types.ReadProofWithPublicInputs("data/" + circuitName + "/proof_with_public_inputs.json"))
	verifierOnlyCircuitData := variables.DeserializeVerifierOnlyCircuitData(types.ReadVerifierOnlyCircuitData("data/" + circuitName + "/verifier_only_circuit_data.json"))
	circuit := verifierCircuit.VerifierCircuit{
		Proof:                   proofWithPis.Proof,
		PublicInputs:            proofWithPis.PublicInputs,
		VerifierOnlyCircuitData: verifierOnlyCircuitData,
		CommonCircuitData:       commonCircuitData,
	}
	builder := scs.NewBuilder
	ccs, err := frontend.Compile(ecc.BN254.ScalarField(), builder, &circuit)
	if err != nil {
		panic(err)
	}
	return ccs
}

func main() {
	circuitName := flag.String("circuit", "", "circuit name")
	flag.Parse()

	if *circuitName == "" {
		fmt.Println("Please provide circuit name")
		os.Exit(1)
	}

	r1cs := loadCircuit(*circuitName)

	proofWithPis := variables.DeserializeProofWithPublicInputs(types.ReadProofWithPublicInputs("data/" + *circuitName + "/proof_with_public_inputs.json"))
	verifierOnlyCircuitData := variables.DeserializeVerifierOnlyCircuitData(types.ReadVerifierOnlyCircuitData("data/" + *circuitName + "/verifier_only_circuit_data.json"))

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
		Proof:                   proofWithPis.Proof,
		PublicInputs:            proofWithPis.PublicInputs,
		VerifierOnlyCircuitData: verifierOnlyCircuitData,
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
		fSol, _ := os.Create("verifier.sol")
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
