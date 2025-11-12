from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_batch_deposit_liquidity_for_perps_response import ApiResponseForBatchDepositLiquidityForPerpsResponse
from ...models.batch_deposit_liquidity_for_perps_request import BatchDepositLiquidityForPerpsRequest
from typing import cast



def _get_kwargs(
    *,
    body: BatchDepositLiquidityForPerpsRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/batch_deposit_liquidity_for_perps",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForBatchDepositLiquidityForPerpsResponse:
    if response.status_code == 200:
        response_200 = ApiResponseForBatchDepositLiquidityForPerpsResponse.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForBatchDepositLiquidityForPerpsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchDepositLiquidityForPerpsRequest,

) -> Response[Any | ApiResponseForBatchDepositLiquidityForPerpsResponse]:
    """  Deposits liquidity for multiple perpetual contracts in a batch operation.

    Processes multiple liquidity deposits, each with their own perp ID and margin amount. Returns
    detailed results for each deposit attempt.

    Args:
        body (BatchDepositLiquidityForPerpsRequest): Batch deposit liquidity for multiple
            perpetual contracts

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchDepositLiquidityForPerpsResponse]
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
    body: BatchDepositLiquidityForPerpsRequest,

) -> Any | ApiResponseForBatchDepositLiquidityForPerpsResponse | None:
    """  Deposits liquidity for multiple perpetual contracts in a batch operation.

    Processes multiple liquidity deposits, each with their own perp ID and margin amount. Returns
    detailed results for each deposit attempt.

    Args:
        body (BatchDepositLiquidityForPerpsRequest): Batch deposit liquidity for multiple
            perpetual contracts

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchDepositLiquidityForPerpsResponse
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchDepositLiquidityForPerpsRequest,

) -> Response[Any | ApiResponseForBatchDepositLiquidityForPerpsResponse]:
    """  Deposits liquidity for multiple perpetual contracts in a batch operation.

    Processes multiple liquidity deposits, each with their own perp ID and margin amount. Returns
    detailed results for each deposit attempt.

    Args:
        body (BatchDepositLiquidityForPerpsRequest): Batch deposit liquidity for multiple
            perpetual contracts

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchDepositLiquidityForPerpsResponse]
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
    body: BatchDepositLiquidityForPerpsRequest,

) -> Any | ApiResponseForBatchDepositLiquidityForPerpsResponse | None:
    """  Deposits liquidity for multiple perpetual contracts in a batch operation.

    Processes multiple liquidity deposits, each with their own perp ID and margin amount. Returns
    detailed results for each deposit attempt.

    Args:
        body (BatchDepositLiquidityForPerpsRequest): Batch deposit liquidity for multiple
            perpetual contracts

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchDepositLiquidityForPerpsResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
