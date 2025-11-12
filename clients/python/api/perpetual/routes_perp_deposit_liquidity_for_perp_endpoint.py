from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_deposit_liquidity_for_perp_response import ApiResponseForDepositLiquidityForPerpResponse
from ...models.deposit_liquidity_for_perp_request import DepositLiquidityForPerpRequest
from typing import cast



def _get_kwargs(
    *,
    body: DepositLiquidityForPerpRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/deposit_liquidity_for_perp",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForDepositLiquidityForPerpResponse:
    if response.status_code == 200:
        response_200 = ApiResponseForDepositLiquidityForPerpResponse.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForDepositLiquidityForPerpResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: DepositLiquidityForPerpRequest,

) -> Response[Any | ApiResponseForDepositLiquidityForPerpResponse]:
    """  Deposits liquidity for a specific perpetual contract.

    Approves USDC spending and deposits the specified margin amount as liquidity for the given perp ID.
    Returns the maker position ID and transaction hashes.

    Args:
        body (DepositLiquidityForPerpRequest): Deposit liquidity for a perpetual contract

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForDepositLiquidityForPerpResponse]
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
    body: DepositLiquidityForPerpRequest,

) -> Any | ApiResponseForDepositLiquidityForPerpResponse | None:
    """  Deposits liquidity for a specific perpetual contract.

    Approves USDC spending and deposits the specified margin amount as liquidity for the given perp ID.
    Returns the maker position ID and transaction hashes.

    Args:
        body (DepositLiquidityForPerpRequest): Deposit liquidity for a perpetual contract

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForDepositLiquidityForPerpResponse
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: DepositLiquidityForPerpRequest,

) -> Response[Any | ApiResponseForDepositLiquidityForPerpResponse]:
    """  Deposits liquidity for a specific perpetual contract.

    Approves USDC spending and deposits the specified margin amount as liquidity for the given perp ID.
    Returns the maker position ID and transaction hashes.

    Args:
        body (DepositLiquidityForPerpRequest): Deposit liquidity for a perpetual contract

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForDepositLiquidityForPerpResponse]
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
    body: DepositLiquidityForPerpRequest,

) -> Any | ApiResponseForDepositLiquidityForPerpResponse | None:
    """  Deposits liquidity for a specific perpetual contract.

    Approves USDC spending and deposits the specified margin amount as liquidity for the given perp ID.
    Returns the maker position ID and transaction hashes.

    Args:
        body (DepositLiquidityForPerpRequest): Deposit liquidity for a perpetual contract

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForDepositLiquidityForPerpResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
