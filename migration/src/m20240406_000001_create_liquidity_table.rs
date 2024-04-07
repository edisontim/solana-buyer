use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

use super::m20240406_000001_create_pool_table::Pool;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Liquidity::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Liquidity::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Liquidity::Ts).big_unsigned().not_null())
                    .col(
                        ColumnDef::new(Liquidity::TargetTokenLiquidity)
                            .big_unsigned()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Liquidity::SolLiquidity)
                            .big_unsigned()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Liquidity::PoolId).big_unsigned().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-liquidity-pool_id")
                            .from(Liquidity::Table, Liquidity::PoolId)
                            .to(Pool::Table, Pool::Id),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Liquidity::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
enum Liquidity {
    Table,
    Id,
    PoolId,
    Ts,
    TargetTokenLiquidity,
    SolLiquidity,
}
