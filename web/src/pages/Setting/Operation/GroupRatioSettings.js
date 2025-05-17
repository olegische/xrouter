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

export default function GroupRatioSettings(props) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    GroupRatio: '',
    UserUsableGroups: '',
  });
  const refForm = useRef();
  const [inputsRow, setInputsRow] = useState(inputs);

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
        <Form.Section text={t('Настройки групп')}>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Коэффициенты групп')}
                placeholder={t('JSON-текст, где ключ — имя группы, значение — коэффициент')}
                field={'GroupRatio'}
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
                  setInputs({ ...inputs, GroupRatio: value })
                }
              />
            </Col>
          </Row>
          <Row gutter={16}>
            <Col xs={24} sm={16}>
              <Form.TextArea
                label={t('Доступные пользователю группы')}
                placeholder={t('JSON-текст, где ключ — имя группы, значение — описание группы')}
                field={'UserUsableGroups'}
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
                  setInputs({ ...inputs, UserUsableGroups: value })
                }
              />
            </Col>
          </Row>
        </Form.Section>
      </Form>
      <Button onClick={onSubmit}>{t('Сохранить настройки коэффициентов групп')}</Button>
    </Spin>
  );
}
