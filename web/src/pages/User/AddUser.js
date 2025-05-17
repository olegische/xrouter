import React, { useState } from 'react';
import { API, isMobile, showError, showSuccess } from '../../helpers';
import Title from '@douyinfe/semi-ui/lib/es/typography/title';
import { Button, Input, SideSheet, Space, Spin } from '@douyinfe/semi-ui';

const AddUser = (props) => {
  const originInputs = {
    username: '',
    display_name: '',
    password: '',
  };
  const [inputs, setInputs] = useState(originInputs);
  const [loading, setLoading] = useState(false);
  const { username, display_name, password } = inputs;

  const handleInputChange = (name, value) => {
    setInputs((inputs) => ({ ...inputs, [name]: value }));
  };

  const submit = async () => {
    setLoading(true);
    if (inputs.username === '' || inputs.password === '') {
      setLoading(false);
      showError('Имя пользователя и пароль не могут быть пустыми!');
      return;
    }
    const res = await API.post(`/api/user/`, inputs);
    const { success, message } = res.data;
    if (success) {
      showSuccess('Учетная запись пользователя успешно создана!');
      setInputs(originInputs);
      props.refresh();
      props.handleClose();
    } else {
      showError(message);
    }
    setLoading(false);
  };

  const handleCancel = () => {
    props.handleClose();
  };

  return (
    <>
      <SideSheet
        placement={'left'}
        title={<Title level={3}>{'Добавить пользователя'}</Title>}
        headerStyle={{ borderBottom: '1px solid var(--semi-color-border)' }}
        bodyStyle={{ borderBottom: '1px solid var(--semi-color-border)' }}
        visible={props.visible}
        footer={
          <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
            <Space>
              <Button theme='solid' size={'large'} onClick={submit}>
                Отправить
              </Button>
              <Button
                theme='solid'
                size={'large'}
                type={'tertiary'}
                onClick={handleCancel}
              >
                Отмена
              </Button>
            </Space>
          </div>
        }
        closeIcon={null}
        onCancel={() => handleCancel()}
        width={isMobile() ? '100%' : 600}
      >
        <Spin spinning={loading}>
          <Input
            style={{ marginTop: 20 }}
            label='Имя пользователя'
            name='username'
            addonBefore={'Имя пользователя'}
            placeholder={'Пожалуйста, введите имя пользователя'}
            onChange={(value) => handleInputChange('username', value)}
            value={username}
            autoComplete='off'
          />
          <Input
            style={{ marginTop: 20 }}
            addonBefore={'Отображаемое имя'}
            label='Отображаемое имя'
            name='display_name'
            autoComplete='off'
            placeholder={'Пожалуйста, введите отображаемое имя'}
            onChange={(value) => handleInputChange('display_name', value)}
            value={display_name}
          />
          <Input
            style={{ marginTop: 20 }}
            label='Пароль'
            name='password'
            type={'password'}
            addonBefore={'Пароль'}
            placeholder={'Пожалуйста, введите пароль'}
            onChange={(value) => handleInputChange('password', value)}
            value={password}
            autoComplete='off'
          />
        </Spin>
      </SideSheet>
    </>
  );
};

export default AddUser;
