import sqlalchemy
from sqlalchemy.ext.automap import automap_base
from sqlalchemy.orm import Session

# pool {
#     table,
#     id,
#     target_token_mint,
#     target_token_pool_vault,
#     sol_pool_vault,
#     rugged,
#     started_indexing_at,
#     done_indexing,
# }

# liquidity {
#     table,
#     id,
#     pool_id,
#     ts,
#     target_token_liquidity,
#     sol_liquidity,
# }


Base = automap_base()
engine = sqlalchemy.create_engine("sqlite:///../liquidities.db")


Base.prepare(autoload_with=engine)

Pool = Base.classes.pool
Liquidity = Base.classes.liquidity


def get_pnl(session, pool, buy_time: int, sell_at_multiplier: int):
    liquidities = session.execute(sqlalchemy.select(Liquidity).where(Liquidity.pool_id == pool.id).order_by(Liquidity.ts.asc()))
    liquidities = liquidities.all()
    for row in liquidities:
        liquidity = row[0]
        print(liquidity.ts)


session = Session(engine)

stmt = sqlalchemy.select(Pool)
pools = session.execute(stmt)


pool = pools.first()[0]
print(pool)
get_pnl(session, pool, 1, 2)
# pools = pools.all()
# num_pools = len(pools)
# num_unrugged_pools = 0
# for row in pools:
#     pool = row[0]
#     print(pool.rugged)
#     get_pnl(session, pool, 1, 2)
#     if pool.rugged == False:
#         num_unrugged_pools += 1

# print(num_unrugged_pools/ num_pools)
