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
import Text from '@douyinfe/semi-ui/lib/es/typography/text.js';

const GEMINI_SETTING_EXAMPLE = {
  default: 'OFF',
  HARM_CATEGORY_CIVIC_INTEGRITY: 'BLOCK_NONE',
};

const GEMINI_VERSION_EXAMPLE = {
  default: 'v1beta',
};

export default function SettingGeminiModel(props) {
  const { t } = useTranslation();

  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    'gemini.safety_settings': '',
    'gemini.version_settings': '',
    'gemini.supported_imagine_models': [],
    'gemini.thinking_adapter_enabled': false,
    'gemini.thinking_adapter_budget_tokens_percentage': 0.6,
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
          <Form.Section text={t('Настройки Gemini')}>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.TextArea
                  label={t('Настройки безопасности Gemini')}
                  placeholder={
                    t('JSON-текст, например:') +
                    '\n' +
                    JSON.stringify(GEMINI_SETTING_EXAMPLE, null, 2)
                  }
                  field={'gemini.safety_settings'}
                  extraText={t(
                    'default — настройка по умолчанию, можно задать уровень безопасности для каждой категории отдельно',
                  )}
                  autosize={{ minRows: 6, maxRows: 12 }}
                  trigger='blur'
                  stopValidateWithError
                  rules={[
                    {
                      validator: (rule, value) => verifyJSON(value),
                      message: t('Недопустимая строка JSON'),
                    },
                  ]}
                  onChange={(value) =>
                    setInputs({ ...inputs, 'gemini.safety_settings': value })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.TextArea
                  label={t('Настройки версии Gemini')}
                  placeholder={
                    t('JSON-текст, например:') +
                    '\n' +
                    JSON.stringify(GEMINI_VERSION_EXAMPLE, null, 2)
                  }
                  field={'gemini.version_settings'}
                  extraText={t('default — настройка по умолчанию, можно задать версию для каждой модели отдельно')}
                  autosize={{ minRows: 6, maxRows: 12 }}
                  trigger='blur'
                  stopValidateWithError
                  rules={[
                    {
                      validator: (rule, value) => verifyJSON(value),
                      message: t('Недопустимая строка JSON'),
                    },
                  ]}
                  onChange={(value) =>
                    setInputs({ ...inputs, 'gemini.version_settings': value })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.TextArea
                  field={'gemini.supported_imagine_models'}
                  label={t('Поддерживаемые модели для генерации изображений')}
                  placeholder={t('Например:') + '\n' + JSON.stringify(['gemini-2.0-flash-exp-image-generation'], null, 2)}
                  onChange={(value) => setInputs({ ...inputs, 'gemini.supported_imagine_models': value })}
                />
              </Col>
            </Row>
          </Form.Section>

          <Form.Section text={t('Настройки адаптера Gemini thinking')}>
            <Row>
              <Col span={16}>
                <Text>
                  {t(
                    "В отличие от Claude, по умолчанию модель Gemini самостоятельно решает, использовать ли thinking. Даже без включения адаптера модель будет работать, но для учёта стоимости рекомендуется устанавливать цену для модели без суффикса так же, как для thinking."
                  )}
                </Text>
              </Col>
            </Row>
            <Row>
              <Col span={16}>
                <Form.Switch
                  label={t('Включить адаптер Gemini thinking (суффиксы -thinking и -nothinking)')}
                  field={'gemini.thinking_adapter_enabled'}
                  extraText={"Адаптирует суффиксы -thinking и -nothinking"}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      'gemini.thinking_adapter_enabled': value,
                    })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Col span={16}>
                <Text>
                  {t(
                    'Gemini thinking adapter BudgetTokens = MaxTokens * процент BudgetTokens',
                  )}
                </Text>
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.InputNumber
                  label={t('Количество BudgetTokens для модели с суффиксом -thinking (свыше 24576 игнорируется)')}
                  field={'gemini.thinking_adapter_budget_tokens_percentage'}
                  initValue={''}
                  extraText={t('Дробное число от 0.1 до 1')}
                  min={0.1}
                  max={1}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      'gemini.thinking_adapter_budget_tokens_percentage': value,
                    })
                  }
                />
              </Col>
            </Row>
          </Form.Section>

          <Row>
            <Button size='default' onClick={onSubmit}>
              {t('Сохранить')}
            </Button>
          </Row>
        </Form>
      </Spin>
    </>
  );
}
