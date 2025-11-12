from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_string import ApiResponseForString
from ...models.fund_guest_wallet_request import FundGuestWalletRequest
from typing import cast



def _get_kwargs(
    *,
    body: FundGuestWalletRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/fund_guest_wallet",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> ApiResponseForString:
    if response.status_code == 200:
        response_200 = ApiResponseForString.from_dict(response.json())



        return response_200

    response_default = ApiResponseForString.from_dict(response.json())



    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[ApiResponseForString]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: FundGuestWalletRequest,

) -> Response[ApiResponseForString]:
    """  Funds a guest wallet with USDC and ETH.

    Transfers the specified amounts of USDC and ETH from the beaconator wallet to the guest wallet
    address. Validates transfer limits and available balances.

    Args:
        body (FundGuestWalletRequest): Fund a guest wallet with USDC and ETH

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiResponseForString]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = client.get_httpx_client().request(
        **kwargs,
    )

    return _build_response(client=client, response=response)

def sync(
    *,
    client: AuthenticatedClient,
    body: FundGuestWalletRequest,

) -> ApiResponseForString | None:
    """  Funds a guest wallet with USDC and ETH.

    Transfers the specified amounts of USDC and ETH from the beaconator wallet to the guest wallet
    address. Validates transfer limits and available balances.

    Args:
        body (FundGuestWalletRequest): Fund a guest wallet with USDC and ETH

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiResponseForString
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: FundGuestWalletRequest,

) -> Response[ApiResponseForString]:
    """  Funds a guest wallet with USDC and ETH.

    Transfers the specified amounts of USDC and ETH from the beaconator wallet to the guest wallet
    address. Validates transfer limits and available balances.

    Args:
        body (FundGuestWalletRequest): Fund a guest wallet with USDC and ETH

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[ApiResponseForString]
     """


    kwargs = _get_kwargs(
        body=body,

    )

    response = await client.get_async_httpx_client().request(
        **kwargs
    )

    return _build_response(client=client, response=response)

async def asyncio(
    *,
    client: AuthenticatedClient,
    body: FundGuestWalletRequest,

) -> ApiResponseForString | None:
    """  Funds a guest wallet with USDC and ETH.

    Transfers the specified amounts of USDC and ETH from the beaconator wallet to the guest wallet
    address. Validates transfer limits and available balances.

    Args:
        body (FundGuestWalletRequest): Fund a guest wallet with USDC and ETH

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        ApiResponseForString
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
