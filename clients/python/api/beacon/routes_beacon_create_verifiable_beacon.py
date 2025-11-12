from http import HTTPStatus
from typing import Any, cast

import httpx

from ...client import AuthenticatedClient, Client
from ...types import Response, UNSET
from ... import errors

from ...models.api_response_for_string import ApiResponseForString
from ...models.create_verifiable_beacon_request import CreateVerifiableBeaconRequest
from typing import cast



def _get_kwargs(
    *,
    body: CreateVerifiableBeaconRequest,

) -> dict[str, Any]:
    headers: dict[str, Any] = {}


    

    

    _kwargs: dict[str, Any] = {
        "method": "post",
        "url": "/create_verifiable_beacon",
    }

    _kwargs["json"] = body.to_dict()


    headers["Content-Type"] = "application/json"

    _kwargs["headers"] = headers
    return _kwargs



def _parse_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Any | ApiResponseForString:
    if response.status_code == 200:
        response_200 = ApiResponseForString.from_dict(response.json())



        return response_200

    response_default = cast(Any, None)
    return response_default



def _build_response(*, client: AuthenticatedClient | Client, response: httpx.Response) -> Response[Any | ApiResponseForString]:
    return Response(
        status_code=HTTPStatus(response.status_code),
        content=response.content,
        headers=response.headers,
        parsed=_parse_response(client=client, response=response),
    )


def sync_detailed(
    *,
    client: AuthenticatedClient,
    body: CreateVerifiableBeaconRequest,

) -> Response[Any | ApiResponseForString]:
    """  Creates a verifiable beacon with Halo2 proof verification.

    Creates a new verifiable beacon using the DichotomousBeaconFactory with the specified verifier
    contract address, initial data value, and TWAP cardinality.

    Args:
        body (CreateVerifiableBeaconRequest): Create a verifiable beacon with zero-knowledge proof
            support and TWAP

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForString]
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
    body: CreateVerifiableBeaconRequest,

) -> Any | ApiResponseForString | None:
    """  Creates a verifiable beacon with Halo2 proof verification.

    Creates a new verifiable beacon using the DichotomousBeaconFactory with the specified verifier
    contract address, initial data value, and TWAP cardinality.

    Args:
        body (CreateVerifiableBeaconRequest): Create a verifiable beacon with zero-knowledge proof
            support and TWAP

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForString
     """


    return sync_detailed(
        client=client,
body=body,

    ).parsed

async def asyncio_detailed(
    *,
    client: AuthenticatedClient,
    body: CreateVerifiableBeaconRequest,

) -> Response[Any | ApiResponseForString]:
    """  Creates a verifiable beacon with Halo2 proof verification.

    Creates a new verifiable beacon using the DichotomousBeaconFactory with the specified verifier
    contract address, initial data value, and TWAP cardinality.

    Args:
        body (CreateVerifiableBeaconRequest): Create a verifiable beacon with zero-knowledge proof
            support and TWAP

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Response[Any | ApiResponseForString]
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
    body: CreateVerifiableBeaconRequest,

) -> Any | ApiResponseForString | None:
    """  Creates a verifiable beacon with Halo2 proof verification.

    Creates a new verifiable beacon using the DichotomousBeaconFactory with the specified verifier
    contract address, initial data value, and TWAP cardinality.

    Args:
        body (CreateVerifiableBeaconRequest): Create a verifiable beacon with zero-knowledge proof
            support and TWAP

    Raises:
        errors.UnexpectedStatus: If the server returns an undocumented status code and Client.raise_on_unexpected_status is True.
        httpx.TimeoutException: If the request takes longer than Client.timeout.

    Returns:
        Any | ApiResponseForString
     """


    return (await asyncio_detailed(
        client=client,
body=body,

    )).parsed
