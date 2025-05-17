import React, { useEffect, useState, useRef } from 'react';
import { Button, Col, Form, Row, Spin, DatePicker } from '@douyinfe/semi-ui';
import dayjs from 'dayjs';
import { useTranslation } from 'react-i18next';
import {
  compareObjects,
  API,
  showError,
  showSuccess,
  showWarning,
} from '../../../helpers';

export default function SettingsLog(props) {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(false);
  const [loadingCleanHistoryLog, setLoadingCleanHistoryLog] = useState(false);
  const [inputs, setInputs] = useState({
    LogConsumeEnabled: false,
    historyTimestamp: dayjs().subtract(1, 'month').toDate(),
  });
  const refForm = useRef();
  const [inputsRow, setInputsRow] = useState(inputs);

  function onSubmit() {
    const updateArray = compareObjects(inputs, inputsRow).filter(
      (item) => item.key !== 'historyTimestamp',
    );

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
  async function onCleanHistoryLog() {
    try {
      setLoadingCleanHistoryLog(true);
      if (!inputs.historyTimestamp) throw new Error(t('Пожалуйста, выберите время для очистки логов'));
      const res = await API.delete(
        `/api/log/?target_timestamp=${Date.parse(inputs.historyTimestamp) / 1000}`,
      );
      const { success, message, data } = res.data;
      if (success) {
        showSuccess(`${data} ${t('логов было очищено!')}`);
        return;
      } else {
        throw new Error(t('Не удалось очистить логи:') + message);
      }
    } catch (error) {
      showError(error.message);
    } finally {
      setLoadingCleanHistoryLog(false);
    }
  }

  useEffect(() => {
    const currentInputs = {};
    for (let key in props.options) {
      if (Object.keys(inputs).includes(key)) {
        currentInputs[key] = props.options[key];
      }
    }
    currentInputs['historyTimestamp'] = inputs.historyTimestamp;
    setInputs(Object.assign(inputs, currentInputs));
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
          <Form.Section text={t('Настройки логов')}>
            <Row gutter={16}>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.Switch
                  field={'LogConsumeEnabled'}
                  label={t('Включить запись логов по расходу лимита')}
                  size='default'
                  checkedText='｜'
                  uncheckedText='〇'
                  onChange={(value) => {
                    setInputs({
                      ...inputs,
                      LogConsumeEnabled: value,
                    });
                  }}
                />
              </Col>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Spin spinning={loadingCleanHistoryLog}>
                  <Form.DatePicker
                    label={t('Время лога')}
                    field={'historyTimestamp'}
                    type='dateTime'
                    inputReadOnly={true}
                    onChange={(value) => {
                      setInputs({
                        ...inputs,
                        historyTimestamp: value,
                      });
                    }}
                  />
                  <Button size='default' onClick={onCleanHistoryLog}>
                    {t('Очистить историю логов')}
                  </Button>
                </Spin>
              </Col>
            </Row>

            <Row>
              <Button size='default' onClick={onSubmit}>
                {t('Сохранить настройки логов')}
              </Button>
            </Row>
          </Form.Section>
        </Form>
      </Spin>
    </>
  );
}
