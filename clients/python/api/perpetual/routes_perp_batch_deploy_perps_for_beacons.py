from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_batch_deploy_perps_for_beacons_response import ApiResponseForBatchDeployPerpsForBeaconsResponse
from ...models.batch_deploy_perps_for_beacons_request import BatchDeployPerpsForBeaconsRequest
from typing import cast



def _get_kwargs(
    *,
    body: BatchDeployPerpsForBeaconsRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/batch_deploy_perps_for_beacons",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForBatchDeployPerpsForBeaconsResponse:
    if response.status_code == 200:
        response_200 = ApiResponseForBatchDeployPerpsForBeaconsResponse.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForBatchDeployPerpsForBeaconsResponse]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchDeployPerpsForBeaconsRequest,

) -> Response[Any | ApiResponseForBatchDeployPerpsForBeaconsResponse]:
    """  Deploys perpetual contracts for multiple beacons in a batch operation.

    Creates perpetual pools for each specified beacon address using the PerpManager contract. Returns
    detailed results including perp IDs for successful deployments.

    Args:
        body (BatchDeployPerpsForBeaconsRequest): Batch deploy perpetual contracts for multiple
            beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchDeployPerpsForBeaconsResponse]
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
    body: BatchDeployPerpsForBeaconsRequest,

) -> Any | ApiResponseForBatchDeployPerpsForBeaconsResponse | None:
    """  Deploys perpetual contracts for multiple beacons in a batch operation.

    Creates perpetual pools for each specified beacon address using the PerpManager contract. Returns
    detailed results including perp IDs for successful deployments.

    Args:
        body (BatchDeployPerpsForBeaconsRequest): Batch deploy perpetual contracts for multiple
            beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchDeployPerpsForBeaconsResponse
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: BatchDeployPerpsForBeaconsRequest,

) -> Response[Any | ApiResponseForBatchDeployPerpsForBeaconsResponse]:
    """  Deploys perpetual contracts for multiple beacons in a batch operation.

    Creates perpetual pools for each specified beacon address using the PerpManager contract. Returns
    detailed results including perp IDs for successful deployments.

    Args:
        body (BatchDeployPerpsForBeaconsRequest): Batch deploy perpetual contracts for multiple
            beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForBatchDeployPerpsForBeaconsResponse]
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
    body: BatchDeployPerpsForBeaconsRequest,

) -> Any | ApiResponseForBatchDeployPerpsForBeaconsResponse | None:
    """  Deploys perpetual contracts for multiple beacons in a batch operation.

    Creates perpetual pools for each specified beacon address using the PerpManager contract. Returns
    detailed results including perp IDs for successful deployments.

    Args:
        body (BatchDeployPerpsForBeaconsRequest): Batch deploy perpetual contracts for multiple
            beacons

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForBatchDeployPerpsForBeaconsResponse
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
