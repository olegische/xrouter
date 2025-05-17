import React, { useEffect, useState, useRef } from 'react';
import { Button, Col, Form, Row, Spin, Banner } from '@douyinfe/semi-ui';
import {
  compareObjects,
  API,
  showError,
  showSuccess,
  showWarning,
  verifyJSON,
} from '../../../helpers';
import { useTranslation } from 'react-i18next';

export default function SettingGlobalModel(props) {
  const { t } = useTranslation();

  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    'global.pass_through_request_enabled': false,
    'general_setting.ping_interval_enabled': false,
    'general_setting.ping_interval_seconds': 60,
  });
  const refForm = useRef();
  const [inputsRow, setInputsRow] = useState(inputs);

  function onSubmit() {
    const updateArray = compareObjects(inputs, inputsRow);
    if (!updateArray.length) return showWarning(t('Похоже, вы ничего не изменили'));
    const requestQueue = updateArray.map((item) => {
      let value = String(inputs[item.key]);

      return API.put('/api/option/', {
        key: item.key,
        value,
      });
    });
    setLoading(true);
    Promise.all(requestQueue)
      .then((res) => {
        if (requestQueue.length === 1) {
          if (res.includes(undefined)) return;
        } else if (requestQueue.length > 1) {
          if (res.includes(undefined))
            return showError(t('Частично не удалось сохранить, попробуйте снова'));
        }
        showSuccess(t('Успешно сохранено'));
        props.refresh();
      })
      .catch(() => {
        showError(t('Не удалось сохранить, попробуйте снова'));
      })
      .finally(() => {
        setLoading(false);
      });
  }

  useEffect(() => {
    const currentInputs = {};
    for (let key in props.options) {
      if (Object.keys(inputs).includes(key)) {
        currentInputs[key] = props.options[key];
      }
    }
    setInputs(currentInputs);
    setInputsRow(structuredClone(currentInputs));
    refForm.current.setValues(currentInputs);
  }, [props.options]);

  return (
    <>
      <Spin spinning={loading}>
        <Form
          values={inputs}
          getFormApi={(formAPI) => (refForm.current = formAPI)}
          style={{ marginBottom: 15 }}
        >
          <Form.Section text={t('Глобальные настройки')}>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.Switch
                  label={t('Включить прозрачную передачу запросов')}
                  field={'global.pass_through_request_enabled'}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      'global.pass_through_request_enabled': value,
                    })
                  }
                  extraText={t('После включения все запросы будут напрямую передаваться upstream, без какой-либо обработки (редиректы и адаптация каналов также будут отключены). Используйте с осторожностью.')}
                />
              </Col>
            </Row>
            
            <Form.Section text={t('Настройки поддержания соединения')}>
            <Row style={{ marginTop: 10 }}>
                  <Col span={24}>
                    <Banner 
                      type="warning"
                      description={t('Внимание: если включено поддержание соединения, и после отправки ping произойдет ошибка канала, система не сможет повторить попытку. Если необходимо включить, рекомендуется установить максимально возможный интервал Ping.')}
                    />
                  </Col>
                </Row>
              <Row>
                <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                  <Form.Switch
                    label={t('Включить интервал Ping')}
                    field={'general_setting.ping_interval_enabled'}
                    onChange={(value) => setInputs({ ...inputs, 'general_setting.ping_interval_enabled': value })}
                    extraText={t('После включения будет периодически отправляться ping для поддержания соединения активным')}
                  />
                </Col>
                <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                  <Form.InputNumber
                    label={t('Интервал Ping (сек)')}
                    field={'general_setting.ping_interval_seconds'}
                    onChange={(value) => setInputs({ ...inputs, 'general_setting.ping_interval_seconds': value })}
                    min={1}
                    disabled={!inputs['general_setting.ping_interval_enabled']}
                  />
                </Col>
              </Row>
            </Form.Section>

            <Row>
              <Button size='default' onClick={onSubmit}>
                {t('Сохранить')}
              </Button>
            </Row>
          </Form.Section>
        </Form>
      </Spin>
    </>
  );
}
