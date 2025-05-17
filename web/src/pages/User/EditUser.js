import React, { useEffect, useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { API, isMobile, showError, showSuccess } from '../../helpers';
import { renderQuota, renderQuotaWithPrompt } from '../../helpers/render';
import Title from '@douyinfe/semi-ui/lib/es/typography/title';
import {
  Button,
  Divider,
  Input,
  Modal,
  Select,
  SideSheet,
  Space,
  Spin,
  Typography,
} from '@douyinfe/semi-ui';
import { useTranslation } from 'react-i18next';

const EditUser = (props) => {
  const userId = props.editingUser.id;
  const [loading, setLoading] = useState(true);
  const [addQuotaModalOpen, setIsModalOpen] = useState(false);
  const [addQuotaLocal, setAddQuotaLocal] = useState('');
  const [inputs, setInputs] = useState({
    username: '',
    display_name: '',
    password: '',
    github_id: '',
    oidc_id: '',
    wechat_id: '',
    email: '',
    quota: 0,
    group: 'default',
  });
  const [groupOptions, setGroupOptions] = useState([]);
  const {
    username,
    display_name,
    password,
    github_id,
    oidc_id,
    wechat_id,
    telegram_id,
    email,
    quota,
    group,
  } = inputs;
  const handleInputChange = (name, value) => {
    setInputs((inputs) => ({ ...inputs, [name]: value }));
  };
  const fetchGroups = async () => {
    try {
      let res = await API.get(`/api/group/`);
      setGroupOptions(
        res.data.data.map((group) => ({
          label: group,
          value: group,
        })),
      );
    } catch (error) {
      showError(error.message);
    }
  };
  const navigate = useNavigate();
  const handleCancel = () => {
    props.handleClose();
  };
  const loadUser = async () => {
    setLoading(true);
    let res = undefined;
    if (userId) {
      res = await API.get(`/api/user/${userId}`);
    } else {
      res = await API.get(`/api/user/self`);
    }
    const { success, message, data } = res.data;
    if (success) {
      data.password = '';
      setInputs(data);
    } else {
      showError(message);
    }
    setLoading(false);
  };

  useEffect(() => {
    loadUser().then();
    if (userId) {
      fetchGroups().then();
    }
  }, [props.editingUser.id]);

  const submit = async () => {
    setLoading(true);
    let res = undefined;
    if (userId) {
      let data = { ...inputs, id: parseInt(userId) };
      if (typeof data.quota === 'string') {
        data.quota = parseInt(data.quota);
      }
      res = await API.put(`/api/user/`, data);
    } else {
      res = await API.put(`/api/user/self`, inputs);
    }
    const { success, message } = res.data;
    if (success) {
      showSuccess('Информация о пользователе успешно обновлена!');
      props.refresh();
      props.handleClose();
    } else {
      showError(message);
    }
    setLoading(false);
  };

  const addLocalQuota = () => {
    let newQuota = parseInt(quota) + parseInt(addQuotaLocal);
    setInputs((inputs) => ({ ...inputs, quota: newQuota }));
  };

  const openAddQuotaModal = () => {
    setAddQuotaLocal('0');
    setIsModalOpen(true);
  };

  const { t } = useTranslation();

  return (
    <>
      <SideSheet
        placement={'right'}
        title={<Title level={3}>{t('Редактировать пользователя')}</Title>}
        headerStyle={{ borderBottom: '1px solid var(--semi-color-border)' }}
        bodyStyle={{ borderBottom: '1px solid var(--semi-color-border)' }}
        visible={props.visible}
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Space>
              <Button theme='solid' size={'large'} onClick={submit}>
                {t('Отправить')}
              </Button>
              <Button
                theme='solid'
                size={'large'}
                type={'tertiary'}
                onClick={handleCancel}
              >
                {t('Отмена')}
              </Button>
            </Space>
          </div>
        }
        closeIcon={null}
        onCancel={() => handleCancel()}
        width={isMobile() ? '100%' : 600}
      >
        <Spin spinning={loading}>
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Имя пользователя')}</Typography.Text>
          </div>
          <Input
            label={t('Имя пользователя')}
            name='username'
            placeholder={t('Пожалуйста, введите новое имя пользователя')}
            onChange={(value) => handleInputChange('username', value)}
            value={username}
            autoComplete='new-password'
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Пароль')}</Typography.Text>
          </div>
          <Input
            label={t('Пароль')}
            name='password'
            type={'password'}
            placeholder={t('Пожалуйста, введите новый пароль, минимум 8 символов')}
            onChange={(value) => handleInputChange('password', value)}
            value={password}
            autoComplete='new-password'
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Отображаемое имя')}</Typography.Text>
          </div>
          <Input
            label={t('Отображаемое имя')}
            name='display_name'
            placeholder={t('Пожалуйста, введите новое отображаемое имя')}
            onChange={(value) => handleInputChange('display_name', value)}
            value={display_name}
            autoComplete='new-password'
          />
          {userId && (
            <>
              <div style={{ marginTop: 20 }}>
                <Typography.Text>{t('Группа')}</Typography.Text>
              </div>
              <Select
                placeholder={t('Пожалуйста, выберите группу')}
                name='group'
                fluid
                search
                selection
                allowAdditions
                additionLabel={t(
                  'Пожалуйста, отредактируйте коэффициенты групп на странице системных настроек, чтобы добавить новую группу:'
                )}
                onChange={(value) => handleInputChange('group', value)}
                value={inputs.group}
                autoComplete='new-password'
                optionList={groupOptions}
              />
              <div style={{ marginTop: 20 }}>
                <Typography.Text>{`${t('Оставшаяся квота')}${renderQuotaWithPrompt(quota)}`}</Typography.Text>
              </div>
              <Space>
                <Input
                  name='quota'
                  placeholder={t('Пожалуйста, введите новую оставшуюся квоту')}
                  onChange={(value) => handleInputChange('quota', value)}
                  value={quota}
                  type={'number'}
                  autoComplete='new-password'
                />
                <Button onClick={openAddQuotaModal}>{t('Добавить квоту')}</Button>
              </Space>
            </>
          )}
          <Divider style={{ marginTop: 20 }}>{t('Следующая информация не может быть изменена')}</Divider>
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Связанный аккаунт GitHub')}</Typography.Text>
          </div>
          <Input
            name='github_id'
            value={github_id}
            autoComplete='new-password'
            placeholder={t(
              'Только для чтения. Пользователь должен связать через соответствующую кнопку на странице личных настроек. Изменение невозможно.'
            )}
            readonly
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Связанный аккаунт OIDC')}</Typography.Text>
          </div>
          <Input
            name='oidc_id'
            value={oidc_id}
            placeholder={t(
              'Только для чтения. Пользователь должен связать через соответствующую кнопку на странице личных настроек. Изменение невозможно.'
            )}
            readonly
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Связанный аккаунт WeChat')}</Typography.Text>
          </div>
          <Input
            name='wechat_id'
            value={wechat_id}
            autoComplete='new-password'
            placeholder={t(
              'Только для чтения. Пользователь должен связать через соответствующую кнопку на странице личных настроек. Изменение невозможно.'
            )}
            readonly
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Связанный email-аккаунт')}</Typography.Text>
          </div>
          <Input
            name='email'
            value={email}
            autoComplete='new-password'
            placeholder={t(
              'Только для чтения. Пользователь должен связать через соответствующую кнопку на странице личных настроек. Изменение невозможно.'
            )}
            readonly
          />
          <div style={{ marginTop: 20 }}>
            <Typography.Text>{t('Связанный аккаунт Telegram')}</Typography.Text>
          </div>
          <Input
            name='telegram_id'
            value={telegram_id}
            autoComplete='new-password'
            placeholder={t(
              'Только для чтения. Пользователь должен связать через соответствующую кнопку на странице личных настроек. Изменение невозможно.'
            )}
            readonly
          />
        </Spin>
      </SideSheet>
      <Modal
        centered={true}
        visible={addQuotaModalOpen}
        onOk={() => {
          addLocalQuota();
          setIsModalOpen(false);
        }}
        onCancel={() => setIsModalOpen(false)}
        closable={null}
      >
        <div style={{ marginTop: 20 }}>
          <Typography.Text>{`${t('Новая квота')}${renderQuota(quota)} + ${renderQuota(addQuotaLocal)} = ${renderQuota(quota + parseInt(addQuotaLocal))}`}</Typography.Text>
        </div>
        <Input
          name='addQuotaLocal'
          placeholder={t('Квота для добавления (поддерживаются отрицательные значения)')}
          onChange={(value) => {
            setAddQuotaLocal(value);
          }}
          value={addQuotaLocal}
          type={'number'}
          autoComplete='new-password'
        />
      </Modal>
    </>
  );
};

export default EditUser;
