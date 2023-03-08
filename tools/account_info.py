import httpx
import json
from constants import GENESIS_ACCOUNT, TEST_ACCOUNT, JSON_RPC_URL
from time import sleep
from xrpl.clients import JsonRpcClient
from xrpl.models.requests.account_info import AccountInfo
from xrpl.models.response import ResponseStatus

# Creates client talking to a node in our testnet.
client = JsonRpcClient(JSON_RPC_URL)


# Gets account information for given account.
def get_account_info(account):
    print(f'Getting account info for {account}')
    request = AccountInfo(
        account=account,
        ledger_index="validated",
        strict=True,
    )
    # Tries to get information about account, ignores connection problems.
    try:
        response = client.request(request)
    except httpx.ConnectError:
        print("Server not responding")
        return False
    print("response.status: ", response.status, flush=True)

    # If request successful, prints information about the account.
    if response.status == ResponseStatus.SUCCESS:
        print(json.dumps(response.result, indent=4, sort_keys=True))
        return True
    else:
        return False


# Gets information about genesis account. This account should exist in 'blank' testnet with
# a balance of 100000000000000000
while not get_account_info(GENESIS_ACCOUNT):
    sleep(10)

# Gets information about test account. This account should not exist in 'blank' testnet and
# this call should return an error. Once the transaction went through, the account should be
# created with a balance of 5000000000
while not get_account_info(TEST_ACCOUNT):
    sleep(10)
