import React, { useEffect, useState, useRef } from 'react';
import { Button, Col, Form, Row, Spin } from '@douyinfe/semi-ui';
import {
  compareObjects,
  API,
  showError,
  showSuccess,
  showWarning,
  verifyJSON,
} from '../../../helpers';
import { useTranslation } from 'react-i18next';

export default function RequestRateLimit(props) {
  const { t } = useTranslation();

  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    ModelRequestRateLimitEnabled: false,
    ModelRequestRateLimitCount: -1,
    ModelRequestRateLimitSuccessCount: 1000,
    ModelRequestRateLimitDurationMinutes: 1,
    ModelRequestRateLimitGroup: '',
  });
  const refForm = useRef();
  const [inputsRow, setInputsRow] = useState(inputs);

  function onSubmit() {
    const updateArray = compareObjects(inputs, inputsRow);
    if (!updateArray.length) return showWarning(t('Похоже, вы ничего не изменили'));
    const requestQueue = updateArray.map((item) => {
      let value = '';
      if (typeof inputs[item.key] === 'boolean') {
        value = String(inputs[item.key]);
      } else {
        value = inputs[item.key];
      }
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

      for (let i = 0; i < res.length; i++) {
        if (!res[i].data.success) {
          return showError(res[i].data.message);
        }
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
          <Form.Section text={t('Ограничение скорости запросов к модели')}>
            <Row gutter={16}>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.Switch
                  field={'ModelRequestRateLimitEnabled'}
                  label={t('Включить ограничение скорости запросов пользователей к модели (может повлиять на производительность при высокой нагрузке)')}
                  size='default'
                  checkedText='｜'
                  uncheckedText='〇'
                  onChange={(value) => {
                    setInputs({
                      ...inputs,
                      ModelRequestRateLimitEnabled: value,
                    });
                  }}
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.InputNumber
                  label={t('Период ограничения')}
                  step={1}
                  min={0}
                  suffix={t('минут')}
                  extraText={t('Период ограничения частоты (в минутах)')}
                  field={'ModelRequestRateLimitDurationMinutes'}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      ModelRequestRateLimitDurationMinutes: String(value),
                    })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.InputNumber
                  label={t('Максимальное количество запросов пользователя за период')}
                  step={1}
                  min={0}
                  suffix={t('раз')}
                  extraText={t('Включая неудачные запросы, 0 — без ограничений')}
                  field={'ModelRequestRateLimitCount'}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      ModelRequestRateLimitCount: String(value),
                    })
                  }
                />
              </Col>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.InputNumber
                  label={t('Максимальное количество успешных запросов пользователя за период')}
                  step={1}
                  min={1}
                  suffix={t('раз')}
                  extraText={t('Только успешные запросы')}
                  field={'ModelRequestRateLimitSuccessCount'}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      ModelRequestRateLimitSuccessCount: String(value),
                    })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={16}>
                <Form.TextArea
                  label={t('Ограничение скорости по группам')}
                  placeholder={t(
                    '{\n  "default": [200, 100],\n  "vip": [0, 1000]\n}',
                  )}
                  field={'ModelRequestRateLimitGroup'}
                autosize={{ minRows: 5, maxRows: 15 }}
                trigger='blur'
                        stopValidateWithError
                rules={[
                  {
                  validator: (rule, value) => verifyJSON(value),
                  message: t('Недопустимая строка JSON'),
                  },
                ]}
                  extraText={
                    <div>
                      <p style={{ marginBottom: -15 }}>{t('Примечание:')}</p>
                      <ul>
                        <li>{t('Используйте формат JSON-объекта: {"имя_группы": [макс. запросов, макс. успешных запросов]}')}</li>
                      <li>{t('Пример: {"default": [200, 100], "vip": [0, 1000]}.')}</li>
                      <li>{t('[макс. запросов] должен быть не меньше 0, [макс. успешных запросов] — не меньше 1.')}</li>
                        <li>{t('Настройки скорости для группы имеют приоритет над глобальными ограничениями.')}</li>
                        <li>{t('Период ограничения всегда соответствует значению "Период ограничения" выше.')}</li>
                      </ul>
                    </div>
                  }
                  onChange={(value) => {
                    setInputs({ ...inputs, ModelRequestRateLimitGroup: value });
                  }}
                />
              </Col>
            </Row>
            <Row>
              <Button size='default' onClick={onSubmit}>
                {t('Сохранить ограничения скорости модели')}
              </Button>
            </Row>
          </Form.Section>
        </Form>
      </Spin>
    </>
  );
}
