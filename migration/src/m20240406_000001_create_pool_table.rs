use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Pool::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Pool::Id)
                            .integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Pool::TargetTokenMint).string().not_null())
                    .col(
                        ColumnDef::new(Pool::TargetTokenPoolVault)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Pool::SolPoolVault).string().not_null())
                    .col(ColumnDef::new(Pool::Rugged).boolean().not_null())
                    .col(ColumnDef::new(Pool::DoneIndexing).boolean().not_null())
                    .col(
                        ColumnDef::new(Pool::StartedIndexingAt)
                            .big_unsigned()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Pool::Table).to_owned())
            .await
    }
}

#[derive(Iden)]
pub enum Pool {
    Table,
    Id,
    TargetTokenMint,
    TargetTokenPoolVault,
    SolPoolVault,
    Rugged,
    StartedIndexingAt,
    DoneIndexing,
}
