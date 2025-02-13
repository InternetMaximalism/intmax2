# GNARK Server

## Setup 

#### Withdrawal 

```bash
go run setup/main.go --circuit=withdrawal_circuit_data
```

#### Claim

```bash
go run setup/main.go --circuit=claim_circuit_data
```

## Run

```bash
# env
cp .env.example .env
```

#### Withdrawal

```bash
go run main.go --circuit=withdrawal_circuit_data
```

#### Claim

```bash
go run main.go --circuit=claim_circuit_data
```

## APIs

```sh
GNARK_SERVER_URL="http://localhost:8080"

# health check
curl $GNARK_SERVER_URL/health
```

### Wrapper

#### generate proof

```sh
curl -X POST "$GNARK_SERVER_URL/start-proof" \
    -H "Content-Type: application/json" \
    --data-binary @testdata/claim_proof.json
```

The output of the start-proof API is a JSON object with the following structure:

```json
{"jobId":"306a20df-e359-4b3c-b6c6-8a1049b90fde"}
```

#### get proof

```sh
curl "$GNARK_SERVER_URL/get-proof?jobId=306a20df-e359-4b3c-b6c6-8a1049b90fde"
```

The output of the get-proof API is a JSON object with the following structure:

```json
{"success":"true","proof":{"publicInputs":["4079990473","4258702484","2081910035","2691585329","2841914472","799830807","2306176734","3986480224"],"proof":"1437b9568489e95f8409a8f1a287ff3a9ea8c1db9a448d5860b477d762ad2158292d5053672465fafa9c8b4fe0cc4ae98b02e5c3489a93875a7534e8b782bc2a19398db9039dcec152f524935629bc09cfbe0251a9ab8bd4847c706c4bd3385720232cbd6c2c90c69fac170b305731b0030814b88710a83a528bb1ae8263d65c0969cc570de7116cb5ad1a9187a629f13ad5599676f30c197d11c002aed7a2f01880c50c16200292fa5d7f5be3e23783facfa09753c4f3522da29af2ecce7c8010bd77229d93a52bdef4b37edceb97080d1beda687b9275df7fae956194bc3a8283314cd6e339dd88897130b525c28856f4e6df4d8f04630a0414ad4414b7bf217af54ee54a5f340b7ee41838fd48ea35456cb24b577293b29ea8d928d4af6ec1036165c18d063d09cb08fb5a0e7c178ca5a2a41161d5d65b62af4c959980a0e1dd0945b0316ffae5de0e6c030c28e3a5a3072a19a50bac8570ab687ed200c8827aa5a4f48b9ce6c4206f1461e24c197169a8c8cccbee03cb5d64e7ae60f3c801bfda7f868e7037e15ab50e66efb4ba027db334c72eecd1f6aa336a12ac58537148cdc6bc69d8522381712a0f852840dd99899c5e4af2de25514f8afd46ad1350208bb399ae41726074635a65b92e8bde37d39fba6f8bc3253f9dddbc5a556ca194a5291a327345002802b59dbd5d5c80d6fc7a03c20e2392f89068f00e924651f940e09b7b66151c8b5c4dde268f8de4c12cc20b310f463d02372d8129cd33b0f97143b335f5511886152e92303bddd54206ec9824762c7f43e847e7bdd895302914638aa57888d7471a596f208455b5a7ce3a887f1c0621035ee4623e575722e53fb36ebf31ef12b6679e328e1f30da484f8f45d885af763c6ee0cfa9e920328b5f056a60c69358b6bf545c31b6758c68241fed06eafefb9527ab76a04128e004e3915643b46e2339ca8da57c3f1dd2089b5dab7d7b9916989ea63821d30260a285e58380bb61b6e18930f21d030b7bcb79e58fcff65127457329471f6ca88171eb0b7dcfd3a4495b8017125cf0ec0052d19b1dcd11c176cdc40f3508462cf10c010706c0d7a88a9998043e722820e7eae8b3deb44de6919fffc01e5b80d282acda869b9decf824a9c946bd4a5a74219821f7118d3458102f21a4e585bddae1faf7843c99f178698414866468f96d08988ccb38bb2cc98c28c1c0c75be5ce914e5b58e6d9a1d8544b64dbab1311ebc3b4f378113885bd8f6f26979ef0ecf672a87ded6e41c681be469185dd57d1a4e532190ffc2a3cb3ecfff56df95e39693"},"errorMessage":null}
```
