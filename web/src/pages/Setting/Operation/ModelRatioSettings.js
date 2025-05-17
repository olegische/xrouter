import React, { useEffect, useState, useRef } from 'react';
import {
  Button,
  Col,
  Form,
  Popconfirm,
  Row,
  Space,
  Spin,
} from '@douyinfe/semi-ui';
import {
  compareObjects,
  API,
  showError,
  showSuccess,
  showWarning,
  verifyJSON,
} from '../../../helpers';
import { useTranslation } from 'react-i18next';

export default function ModelRatioSettings(props) {
  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    ModelPrice: '',
    ModelRatio: '',
    CacheRatio: '',
    CompletionRatio: '',
  });
  const refForm = useRef();
  const [inputsRow, setInputsRow] = useState(inputs);
  const { t } = useTranslation();

  async function onSubmit() {
    try {
      await refForm.current
        .validate()
        .then(() => {
          const updateArray = compareObjects(inputs, inputsRow);
          if (!updateArray.length)
            return showWarning(t('Похоже, вы ничего не изменили'));

          const requestQueue = updateArray.map((item) => {
            const value =
              typeof inputs[item.key] === 'boolean'
                ? String(inputs[item.key])
                : inputs[item.key];
            return API.put('/api/option/', { key: item.key, value });
          });

          setLoading(true);
          Promise.all(requestQueue)
            .then((res) => {
              if (res.includes(undefined)) {
                return showError(
                  requestQueue.length > 1
                    ? t('Частично не удалось сохранить, попробуйте снова')
                    : t('Не удалось сохранить'),
                );
              }

              for (let i = 0; i < res.length; i++) {
                if (!res[i].data.success) {
                  return showError(res[i].data.message);
                }
              }

              showSuccess(t('Успешно сохранено'));
              props.refresh();
            })
            .catch((error) => {
              console.error('Unexpected error:', error);
              showError(t('Не удалось сохранить, попробуйте снова'));
            })
            .finally(() => {
              setLoading(false);
            });
        })
        .catch(() => {
          showError(t('Проверьте введённые данные'));
        });
    } catch (error) {
      showError(t('Проверьте введённые данные'));
      console.error(error);
    }
  }

  async function resetModelRatio() {
    try {
      let res = await API.post(`/api/option/rest_model_ratio`);
      if (res.data.success) {
        showSuccess(res.data.message);
        props.refresh();
      } else {
        showError(res.data.message);
      }
    } catch (error) {
      showError(error);
    }
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
    <Spin spinning={loading}>
      <Form
        values={inputs}
        getFormApi={(formAPI) => (refForm.current = formAPI)}
        style={{ marginBottom: 15 }}
      >
        <Form.Section>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Фиксированная цена модели')}
                extraText={t('Сколько стоит один вызов, приоритет выше, чем у коэффициента модели')}
                placeholder={t(
                  'JSON-текст, где ключ — имя модели, значение — стоимость одного вызова, например "gpt-4-gizmo-*": 0.1, один вызов стоит 0.1$',
                )}
                field={'ModelPrice'}
                autosize={{ minRows: 6, maxRows: 12 }}
                trigger='blur'
                stopValidateWithError
                rules={[
                  {
                    validator: (rule, value) => verifyJSON(value),
                    message: 'Недопустимая строка JSON',
                  },
                ]}
                onChange={(value) =>
                  setInputs({ ...inputs, ModelPrice: value })
                }
              />
            </Col>
          </Row>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Коэффициент модели')}
                placeholder={t('JSON-текст, где ключ — имя модели, значение — коэффициент')}
                field={'ModelRatio'}
                autosize={{ minRows: 6, maxRows: 12 }}
                trigger='blur'
                stopValidateWithError
                rules={[
                  {
                    validator: (rule, value) => verifyJSON(value),
                    message: 'Недопустимая строка JSON',
                  },
                ]}
                onChange={(value) =>
                  setInputs({ ...inputs, ModelRatio: value })
                }
              />
            </Col>
          </Row>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Коэффициент кэширования подсказок')}
                placeholder={t('JSON-текст, где ключ — имя модели, значение — коэффициент')}
                field={'CacheRatio'}
                autosize={{ minRows: 6, maxRows: 12 }}
                trigger='blur'
                stopValidateWithError
                rules={[
                  {
                    validator: (rule, value) => verifyJSON(value),
                    message: 'Недопустимая строка JSON',
                  },
                ]}
                onChange={(value) =>
                  setInputs({ ...inputs, CacheRatio: value })
                }
              />
            </Col>
          </Row>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Коэффициент автодополнения модели (только для пользовательских моделей)')}
                extraText={t('Только для пользовательских моделей')}
                placeholder={t('JSON-текст, где ключ — имя модели, значение — коэффициент')}
                field={'CompletionRatio'}
                autosize={{ minRows: 6, maxRows: 12 }}
                trigger='blur'
                stopValidateWithError
                rules={[
                  {
                    validator: (rule, value) => verifyJSON(value),
                    message: 'Недопустимая строка JSON',
                  },
                ]}
                onChange={(value) =>
                  setInputs({ ...inputs, CompletionRatio: value })
                }
              />
            </Col>
          </Row>
        </Form.Section>
      </Form>
      <Space>
        <Button onClick={onSubmit}>{t('Сохранить настройки коэффициентов модели')}</Button>
        <Popconfirm
          title={t('Вы уверены, что хотите сбросить коэффициенты модели?')}
          content={t('Это действие необратимо')}
          okType={'danger'}
          position={'top'}
          onConfirm={resetModelRatio}
        >
          <Button type={'danger'}>{t('Сбросить коэффициенты модели')}</Button>
        </Popconfirm>
      </Space>
    </Spin>
  );
}
