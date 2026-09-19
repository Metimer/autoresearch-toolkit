from workload import workload

assert workload()[0] == [sum(i * i for i in range(n % 20 + 1)) for n in range(200)]
