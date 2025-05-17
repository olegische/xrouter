import React, { useContext, useEffect, useState, useRef } from 'react';
import {
  Card,
  Col,
  Row,
  Form,
  Button,
  Typography,
  Space,
  RadioGroup,
  Radio,
  Modal,
  Banner,
} from '@douyinfe/semi-ui';
import { API, showError, showNotice, timestamp2string } from '../../helpers';
import { StatusContext } from '../../context/Status';
import { marked } from 'marked';
import { StyleContext } from '../../context/Style/index.js';
import { useTranslation } from 'react-i18next';
import {
  IconHelpCircle,
  IconInfoCircle,
  IconAlertTriangle,
} from '@douyinfe/semi-icons';

const Setup = () => {
  const { t, i18n } = useTranslation();
  const [statusState] = useContext(StatusContext);
  const [styleState, styleDispatch] = useContext(StyleContext);
  const [loading, setLoading] = useState(false);
  const [selfUseModeInfoVisible, setUsageModeInfoVisible] = useState(false);
  const [setupStatus, setSetupStatus] = useState({
    status: false,
    root_init: false,
    database_type: '',
  });
  const { Text, Title } = Typography;
  const formRef = useRef(null);

  const [formData, setFormData] = useState({
    username: '',
    password: '',
    confirmPassword: '',
    usageMode: 'external',
  });

  useEffect(() => {
    fetchSetupStatus();
  }, []);

  const fetchSetupStatus = async () => {
    try {
      const res = await API.get('/api/setup');
      const { success, data } = res.data;
      if (success) {
        setSetupStatus(data);

        // If setup is already completed, redirect to home
        if (data.status) {
          window.location.href = '/';
        }
      } else {
        showError(t('Не удалось получить статус инициализации'));
      }
    } catch (error) {
      console.error('Failed to fetch setup status:', error);
      showError(t('Не удалось получить статус инициализации'));
    }
  };

  const handleUsageModeChange = (val) => {
    setFormData({ ...formData, usageMode: val });
  };

  const onSubmit = () => {
    if (!formRef.current) {
      console.error('Form reference is null');
      showError(t('Ошибка ссылки на форму, пожалуйста, обновите страницу и попробуйте снова'));
      return;
    }

    const values = formRef.current.getValues();
    console.log('Form values:', values);

    // For root_init=false, validate admin username and password
    if (!setupStatus.root_init) {
      if (!values.username || !values.username.trim()) {
        showError(t('Пожалуйста, введите имя пользователя администратора'));
        return;
      }

      if (!values.password || values.password.length < 8) {
        showError(t('Пароль должен содержать не менее 8 символов'));
        return;
      }

      if (values.password !== values.confirmPassword) {
        showError(t('Введённые пароли не совпадают'));
        return;
      }
    }

    // Prepare submission data
    const formValues = { ...values };
    formValues.SelfUseModeEnabled = values.usageMode === 'self';
    formValues.DemoSiteEnabled = values.usageMode === 'demo';

    // Remove usageMode as it's not needed by the backend
    delete formValues.usageMode;

    console.log('Submitting data to backend:', formValues);
    setLoading(true);

    // Submit to backend
    API.post('/api/setup', formValues)
      .then((res) => {
        const { success, message } = res.data;
        console.log('API response:', res.data);

        if (success) {
          showNotice(t('Система успешно инициализирована, перенаправление...'));
          setTimeout(() => {
            window.location.reload();
          }, 1500);
        } else {
          showError(message || t('Инициализация не удалась, попробуйте снова'));
        }
      })
      .catch((error) => {
        console.error('API error:', error);
        showError(t('Не удалось инициализировать систему, попробуйте снова'));
        setLoading(false);
      })
      .finally(() => {
        // setLoading(false);
      });
  };

  return (
    <>
      <div style={{ maxWidth: '800px', margin: '0 auto', padding: '20px' }}>
        <Card>
          <Title heading={2} style={{ marginBottom: '24px' }}>
            {t('Инициализация системы')}
          </Title>

          {setupStatus.database_type === 'sqlite' && (
            <Banner
              type='warning'
              icon={<IconAlertTriangle size='large' />}
              closeIcon={null}
              title={t('Предупреждение базы данных')}
              description={
                <div>
                  <p>
                    {t(
                      'Вы используете базу данных SQLite. Если вы работаете в контейнере, убедитесь, что файл базы данных правильно смонтирован для постоянного хранения, иначе все данные будут потеряны после перезапуска контейнера!'
                    )}
                  </p>
                  <p>
                    {t(
                      'Рекомендуется использовать MySQL или PostgreSQL в производственной среде, либо убедиться, что файл SQLite смонтирован на постоянное хранилище хоста.'
                    )}
                  </p>
                </div>
              }
              style={{ marginBottom: '24px' }}
            />
          )}

          <Form
            getFormApi={(formApi) => {
              formRef.current = formApi;
              console.log('Form API set:', formApi);
            }}
            initValues={formData}
          >
            {setupStatus.root_init ? (
              <Banner
                type='info'
                icon={<IconInfoCircle />}
                closeIcon={null}
                description={t('Аккаунт администратора уже инициализирован, продолжайте настройку параметров системы')}
                style={{ marginBottom: '24px' }}
              />
            ) : (
              <Form.Section text={t('Аккаунт администратора')}>
                <Form.Input
                  field='username'
                  label={t('Имя пользователя')}
                  placeholder={t('Пожалуйста, введите имя пользователя администратора')}
                  showClear
                  onChange={(value) =>
                    setFormData({ ...formData, username: value })
                  }
                />
                <Form.Input
                  field='password'
                  label={t('Пароль')}
                  placeholder={t('Пожалуйста, введите пароль администратора')}
                  type='password'
                  showClear
                  onChange={(value) =>
                    setFormData({ ...formData, password: value })
                  }
                />
                <Form.Input
                  field='confirmPassword'
                  label={t('Подтвердите пароль')}
                  placeholder={t('Пожалуйста, подтвердите пароль администратора')}
                  type='password'
                  showClear
                  onChange={(value) =>
                    setFormData({ ...formData, confirmPassword: value })
                  }
                />
              </Form.Section>
            )}

            <Form.Section
              text={
                <div style={{ display: 'flex', alignItems: 'center' }}>
                  {t('Настройки системы')}
                </div>
              }
            >
              <Form.RadioGroup
                field='usageMode'
                label={
                  <div style={{ display: 'flex', alignItems: 'center' }}>
                    {t('Режим использования')}
                    <IconHelpCircle
                      style={{
                        marginLeft: '4px',
                        color: 'var(--semi-color-primary)',
                        verticalAlign: 'middle',
                        cursor: 'pointer',
                      }}
                      onClick={(e) => {
                        // e.preventDefault();
                        // e.stopPropagation();
                        setUsageModeInfoVisible(true);
                      }}
                    />
                  </div>
                }
                extraText={t('Можно изменить после инициализации')}
                initValue='external'
                onChange={handleUsageModeChange}
              >
                <Form.Radio value='external'>{t('Режим внешней эксплуатации')}</Form.Radio>
                <Form.Radio value='self'>{t('Режим личного использования')}</Form.Radio>
                <Form.Radio value='demo'>{t('Демонстрационный режим')}</Form.Radio>
              </Form.RadioGroup>
            </Form.Section>
          </Form>

          <div style={{ marginTop: '24px', textAlign: 'right' }}>
            <Button type='primary' onClick={onSubmit} loading={loading}>
              {t('Инициализировать систему')}
            </Button>
          </div>
        </Card>
      </div>

      <Modal
        title={t('Описание режимов использования')}
        visible={selfUseModeInfoVisible}
        onOk={() => setUsageModeInfoVisible(false)}
        onCancel={() => setUsageModeInfoVisible(false)}
        closeOnEsc={true}
        okText={t('ОК')}
        cancelText={null}
      >
        <div style={{ padding: '8px 0' }}>
          <Title heading={6}>{t('Режим внешней эксплуатации')}</Title>
          <p>{t('Режим по умолчанию, подходит для предоставления услуг нескольким пользователям.')}</p>
          <p>
            {t(
              'В этом режиме система будет рассчитывать использование для каждого вызова, необходимо установить цену для каждой модели. Если цена не установлена, пользователи не смогут использовать эту модель.'
            )}
          </p>
        </div>
        <div style={{ padding: '8px 0' }}>
          <Title heading={6}>{t('Режим личного использования')}</Title>
          <p>{t('Подходит для личного использования.')}</p>
          <p>
            {t('Не требуется устанавливать цены на модели, система будет минимизировать учет использования, вы можете сосредоточиться на работе с моделями.')}
          </p>
        </div>
        <div style={{ padding: '8px 0' }}>
          <Title heading={6}>{t('Демонстрационный режим')}</Title>
          <p>{t('Подходит для демонстрации функционала системы.')}</p>
        </div>
      </Modal>
    </>
  );
};

export default Setup;
