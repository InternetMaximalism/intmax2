package handlers

import (
	con "context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"time"

	verifierCircuit "gnark-server/circuit"
	"gnark-server/context"

	"github.com/consensys/gnark-crypto/ecc"
	plonk_bn254 "github.com/consensys/gnark/backend/plonk/bn254"
	"github.com/consensys/gnark/backend/witness"
	"github.com/consensys/gnark/frontend"
	"github.com/google/uuid"
	"github.com/qope/gnark-plonky2-verifier/types"
	"github.com/qope/gnark-plonky2-verifier/variables"
	"github.com/redis/go-redis/v9"
)

type ProveResult struct {
	PublicInputs []string `json:"publicInputs"`
	Proof        string   `json:"proof"`
}

type ProofResponse struct {
	Success      bool         `json:"success"`
	Proof        *ProveResult `json:"proof"`
	ErrorMessage *string      `json:"errorMessage"`
}

type CircuitData struct {
	context.CircuitData
	RedisClient *redis.Client
}

const (
	redisKeyPrefix = "gnark_proof_result:"
	expiration     = 1 * time.Hour
)

func getRedisKey(jobId string) string {
	return fmt.Sprintf("%s%s", redisKeyPrefix, jobId)
}

func (ctx *CircuitData) storeProofResponse(jobId string, response ProofResponse) error {
	jsonData, err := json.Marshal(response)
	if err != nil {
		return fmt.Errorf("failed to marshal proof response: %v", err)
	}

	err = ctx.RedisClient.Set(con.Background(), getRedisKey(jobId), string(jsonData), expiration).Err()
	if err != nil {
		return fmt.Errorf("failed to store in redis: %v", err)
	}
	return nil
}

func extractPublicInputs(witness witness.Witness) ([]*big.Int, error) {
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

func (ctx *CircuitData) prove(jobId string, proofRaw types.ProofWithPublicInputsRaw) error {
	proofWithPis := variables.DeserializeProofWithPublicInputs(proofRaw)
	assignment := verifierCircuit.VerifierCircuit{
		Proof:                   proofWithPis.Proof,
		PublicInputs:            proofWithPis.PublicInputs,
		VerifierOnlyCircuitData: ctx.VerifierOnlyCircuitData,
	}
	witness, err := frontend.NewWitness(&assignment, ecc.BN254.ScalarField())
	if err != nil {
		errMsg := err.Error()
		response := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		if storeErr := ctx.storeProofResponse(jobId, response); storeErr != nil {
			return fmt.Errorf("failed to store error response: %v", storeErr)
		}
		return err
	}
	proof, err := plonk_bn254.Prove(&ctx.Ccs, &ctx.Pk, witness)
	if err != nil {
		errMsg := err.Error()
		response := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		if storeErr := ctx.storeProofResponse(jobId, response); storeErr != nil {
			return fmt.Errorf("failed to store error response: %v", storeErr)
		}
	}
	proofHex := hex.EncodeToString(proof.MarshalSolidity())
	publicInputs, err := extractPublicInputs(witness)
	if err != nil {
		errMsg := err.Error()
		response := ProofResponse{
			Success:      false,
			Proof:        nil,
			ErrorMessage: &errMsg,
		}
		if storeErr := ctx.storeProofResponse(jobId, response); storeErr != nil {
			return fmt.Errorf("failed to store error response: %v", storeErr)
		}
	}
	publicInputsStr := make([]string, len(publicInputs))
	for i, bi := range publicInputs {
		publicInputsStr[i] = bi.String()
	}

	response := ProofResponse{
		Success: true,
		Proof: &ProveResult{
			PublicInputs: publicInputsStr,
			Proof:        proofHex,
		},
	}
	if err := ctx.storeProofResponse(jobId, response); err != nil {
		return fmt.Errorf("failed to store success response: %v", err)
	}

	fmt.Println("Prove done. jobId", jobId)
	return nil
}

func (ctx *CircuitData) StartProof(w http.ResponseWriter, r *http.Request) {
	_jobId, err := uuid.NewRandom()
	if err != nil {
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}
	jobId := _jobId.String()
	var rawInput struct {
		Proof string `json:"proof"`
	}
	if err := json.NewDecoder(r.Body).Decode(&rawInput); err != nil {
		http.Error(w, err.Error(), http.StatusBadRequest)
		return
	}

	var input types.ProofWithPublicInputsRaw
	if err := json.Unmarshal([]byte(rawInput.Proof), &input); err != nil {
		http.Error(w, "Failed to parse proof JSON: "+err.Error(), http.StatusBadRequest)
		return
	}

	response := ProofResponse{Success: true, Proof: nil}
	if err := ctx.storeProofResponse(jobId, response); err != nil {
		fmt.Printf("Failed to store proof response in Redis: %v\n", err)
		return
	}

	go ctx.prove(jobId, input)
	json.NewEncoder(w).Encode(map[string]string{"jobId": jobId})
}

func (ctx *CircuitData) GetProof(w http.ResponseWriter, r *http.Request) {
	jobId := r.URL.Query().Get("jobId")
	_, err := uuid.Parse(jobId)
	if err != nil {
		http.Error(w, "Invalid JobId", http.StatusBadRequest)
		return
	}

	result, err := ctx.RedisClient.Get(con.Background(), getRedisKey(jobId)).Result()
	if err == redis.Nil {
		http.Error(w, "job not found", http.StatusNotFound)
		return
	} else if err != nil {
		http.Error(w, "Failed to get proof status: "+err.Error(), http.StatusInternalServerError)
		return
	}

	var response ProofResponse
	if err := json.Unmarshal([]byte(result), &response); err != nil {
		http.Error(w, "Failed to parse proof response: "+err.Error(), http.StatusInternalServerError)
		return
	}

	json.NewEncoder(w).Encode(response)
}
