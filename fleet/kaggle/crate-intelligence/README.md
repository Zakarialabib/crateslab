# Kaggle crate-intelligence job

Fleet prepares the private `kernel-metadata.json` from `KAGGLE_KERNEL_ID` and
submits this directory only from a review-confirmed action. The job runs on
CPU by default and requires internet to download the pinned private HF index
and the small sentence-transformer model.

Before running, create Kaggle Secrets named `HF_READ_TOKEN`,
`HF_CRATES_DATASET_REPO`, and `HF_CRATES_DATASET_REVISION`. Use a dedicated
read-only HF token. The code never includes credentials. Kaggle returns a
crate-isolated retrieval comparison with aggregate metrics and source paths only.
It does not save source chunks or embeddings as outputs, and Fleet does not
automatically download or publish results to HF.

`kaggle kernels push` uploads a notebook version and starts a run; a status
check is a separate read-only operation. The job is marked private and does
not request a GPU by default. Change resource settings only after a measured
CPU baseline.
