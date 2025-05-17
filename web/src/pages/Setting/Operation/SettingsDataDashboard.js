import React, { useEffect, useState, useRef } from 'react';
import { Button, Col, Form, Row, Spin } from '@douyinfe/semi-ui';
import {
  compareObjects,
  API,
  showError,
  showSuccess,
  showWarning,
} from '../../../helpers';
import { useTranslation } from 'react-i18next';

export default function DataDashboard(props) {
  const { t } = useTranslation();

  const optionsDataExportDefaultTime = [
    { key: 'hour', label: t('Час'), value: 'hour' },
    { key: 'day', label: t('День'), value: 'day' },
    { key: 'week', label: t('Неделя'), value: 'week' },
  ];
  const [loading, setLoading] = useState(false);
  const [inputs, setInputs] = useState({
    DataExportEnabled: false,
    DataExportInterval: '',
    DataExportDefaultTime: '',
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
    localStorage.setItem(
      'data_export_default_time',
      String(inputs.DataExportDefaultTime),
    );
  }, [props.options]);

  return (
    <>
      <Spin spinning={loading}>
        <Form
          values={inputs}
          getFormApi={(formAPI) => (refForm.current = formAPI)}
          style={{ marginBottom: 15 }}
        >
          <Form.Section text={t('Настройки панели данных')}>
            <Row gutter={16}>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.Switch
                  field={'DataExportEnabled'}
                  label={t('Включить панель данных (экспериментально)')}
                  size='default'
                  checkedText='｜'
                  uncheckedText='〇'
                  onChange={(value) => {
                    setInputs({
                      ...inputs,
                      DataExportEnabled: value,
                    });
                  }}
                />
              </Col>
            </Row>
            <Row>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.InputNumber
                  label={t('Интервал обновления панели данных')}
                  step={1}
                  min={1}
                  suffix={t('минут')}
                  extraText={t('Слишком короткий интервал может повлиять на производительность базы данных')}
                  placeholder={t('Интервал обновления панели данных')}
                  field={'DataExportInterval'}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      DataExportInterval: String(value),
                    })
                  }
                />
              </Col>
              <Col xs={24} sm={12} md={8} lg={8} xl={8}>
                <Form.Select
                  label={t('Временная детализация панели данных по умолчанию')}
                  optionList={optionsDataExportDefaultTime}
                  field={'DataExportDefaultTime'}
                  extraText={t('Изменяет только отображение, статистика всегда с точностью до часа')}
                  placeholder={t('Временная детализация панели данных по умолчанию')}
                  style={{ width: 180 }}
                  onChange={(value) =>
                    setInputs({
                      ...inputs,
                      DataExportDefaultTime: String(value),
                    })
                  }
                />
              </Col>
            </Row>
            <Row>
              <Button size='default' onClick={onSubmit}>
                {t('Сохранить настройки панели данных')}
              </Button>
            </Row>
          </Form.Section>
        </Form>
      </Spin>
    </>
  );
}
