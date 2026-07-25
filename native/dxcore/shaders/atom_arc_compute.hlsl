ByteAddressBuffer input_values : register(t0);
RWByteAddressBuffer output_values : register(u0);

cbuffer Parameters : register(b0)
{
    uint addend;
    uint element_count;
};

[numthreads(64, 1, 1)]
void main(uint3 dispatch_thread_id : SV_DispatchThreadID)
{
    const uint index = dispatch_thread_id.x;
    if (index < element_count)
    {
        const uint input_value = input_values.Load(index * 4u);
        const uint value = (input_value + addend) ^ 0xa5a5a5a5u;
        output_values.Store(index * 4u, value);
    }
}
