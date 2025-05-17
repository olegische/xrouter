import React from 'react';
import TokensTable from '../../components/TokensTable';
import { Banner, Layout } from '@douyinfe/semi-ui';
import { useTranslation } from 'react-i18next';
const Token = () => {
  const { t } = useTranslation();
  return (
    <>
      <Layout>
        <Layout.Header>
          <Banner
            type='warning'
            description={t(
              'Токен не может точно контролировать лимит использования, разрешено только для личного использования, не передавайте токен другим лицам напрямую.'
            )}
          />
        </Layout.Header>
        <Layout.Content>
          <TokensTable />
        </Layout.Content>
      </Layout>
    </>
  );
};

export default Token;
