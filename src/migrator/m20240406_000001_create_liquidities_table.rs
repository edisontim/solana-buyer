use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

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
                        ColumnDef::new(Liquidity::TargetTokenMint)
                            .string()
                            .not_null(),
                    )
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
    Ts,
    TargetTokenMint,
    TargetTokenLiquidity,
    SolLiquidity,
}
