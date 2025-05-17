// ModelSettingsVisualEditor.js
import React, { useContext, useEffect, useState, useRef } from 'react';
import {
  Table,
  Button,
  Input,
  Modal,
  Form,
  Space,
  RadioGroup,
  Radio,
  Tabs,
  TabPane,
} from '@douyinfe/semi-ui';
import {
  IconDelete,
  IconPlus,
  IconSearch,
  IconSave,
  IconEdit,
} from '@douyinfe/semi-icons';
import { showError, showSuccess } from '../../../helpers';
import { API } from '../../../helpers';
import { useTranslation } from 'react-i18next';
import { StatusContext } from '../../../context/Status/index.js';
import { getQuotaPerUnit } from '../../../helpers/render.js';

export default function ModelSettingsVisualEditor(props) {
  const { t } = useTranslation();
  const [models, setModels] = useState([]);
  const [visible, setVisible] = useState(false);
  const [currentModel, setCurrentModel] = useState(null);
  const [searchText, setSearchText] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [loading, setLoading] = useState(false);
  const [pricingMode, setPricingMode] = useState('per-token'); // 'per-token' or 'per-request'
  const [pricingSubMode, setPricingSubMode] = useState('ratio'); // 'ratio' or 'token-price'
  const formRef = useRef(null);
  const pageSize = 10;
  const quotaPerUnit = getQuotaPerUnit();

  useEffect(() => {
    try {
      const modelPrice = JSON.parse(props.options.ModelPrice || '{}');
      const modelRatio = JSON.parse(props.options.ModelRatio || '{}');
      const completionRatio = JSON.parse(props.options.CompletionRatio || '{}');

      // Merge all model names from price, ratio, and completionRatio
      const modelNames = new Set([
        ...Object.keys(modelPrice),
        ...Object.keys(modelRatio),
        ...Object.keys(completionRatio),
      ]);

      const modelData = Array.from(modelNames).map((name) => ({
        name,
        price: modelPrice[name] === undefined ? '' : modelPrice[name],
        ratio: modelRatio[name] === undefined ? '' : modelRatio[name],
        completionRatio:
          completionRatio[name] === undefined ? '' : completionRatio[name],
      }));

      setModels(modelData);
    } catch (error) {
      console.error('JSON parse error:', error);
    }
  }, [props.options]);

  // Pagination utility function
  const getPagedData = (data, currentPage, pageSize) => {
    const start = (currentPage - 1) * pageSize;
    const end = start + pageSize;
    return data.slice(start, end);
  };

  // Before the return statement, process filtering and pagination logic
  const filteredModels = models.filter((model) =>
    searchText
      ? model.name.toLowerCase().includes(searchText.toLowerCase())
      : true,
  );

  // Calculate paged data based on filtered models
  const pagedData = getPagedData(filteredModels, currentPage, pageSize);

  const SubmitData = async () => {
    setLoading(true);
    const output = {
      ModelPrice: {},
      ModelRatio: {},
      CompletionRatio: {},
    };
    let currentConvertModelName = '';

    try {
      // Data conversion
      models.forEach((model) => {
        currentConvertModelName = model.name;
        if (model.price !== '') {
          // If price is not empty, convert to float and ignore ratio
          output.ModelPrice[model.name] = parseFloat(model.price);
        } else {
          if (model.ratio !== '')
            output.ModelRatio[model.name] = parseFloat(model.ratio);
          if (model.completionRatio !== '')
            output.CompletionRatio[model.name] = parseFloat(
              model.completionRatio,
            );
        }
      });

      // Prepare API request array
      const finalOutput = {
        ModelPrice: JSON.stringify(output.ModelPrice, null, 2),
        ModelRatio: JSON.stringify(output.ModelRatio, null, 2),
        CompletionRatio: JSON.stringify(output.CompletionRatio, null, 2),
      };

      const requestQueue = Object.entries(finalOutput).map(([key, value]) => {
        return API.put('/api/option/', {
          key,
          value,
        });
      });

      // Batch process requests
      const results = await Promise.all(requestQueue);

      // Validate results
      if (requestQueue.length === 1) {
        if (results.includes(undefined)) return;
      } else if (requestQueue.length > 1) {
        if (results.includes(undefined)) {
          return showError(t('Частично не удалось сохранить, попробуйте снова'));
        }
      }

      // Check each request result
      for (const res of results) {
        if (!res.data.success) {
          return showError(res.data.message);
        }
      }

      showSuccess(t('Успешно сохранено'));
      props.refresh();
    } catch (error) {
      console.error('Save failed:', error);
      showError(t('Не удалось сохранить, попробуйте снова'));
    } finally {
      setLoading(false);
    }
  };

  const columns = [
    {
      title: t('Название модели'),
      dataIndex: 'name',
      key: 'name',
    },
    {
      title: t('Фиксированная цена модели'),
      dataIndex: 'price',
      key: 'price',
      render: (text, record) => (
        <Input
          value={text}
          placeholder={t('Поминутная тарификация')}
          onChange={(value) => updateModel(record.name, 'price', value)}
        />
      ),
    },
    {
      title: t('Коэффициент модели'),
      dataIndex: 'ratio',
      key: 'ratio',
      render: (text, record) => (
        <Input
          value={text}
          placeholder={record.price !== '' ? t('Коэффициент модели') : t('Коэффициент по умолчанию')}
          disabled={record.price !== ''}
          onChange={(value) => updateModel(record.name, 'ratio', value)}
        />
      ),
    },
    {
      title: t('Коэффициент автодополнения'),
      dataIndex: 'completionRatio',
      key: 'completionRatio',
      render: (text, record) => (
        <Input
          value={text}
          placeholder={record.price !== '' ? t('Коэффициент автодополнения') : t('Коэффициент автодополнения по умолчанию')}
          disabled={record.price !== ''}
          onChange={(value) =>
            updateModel(record.name, 'completionRatio', value)
          }
        />
      ),
    },
    {
      title: t('Действия'),
      key: 'action',
      render: (_, record) => (
        <Space>
          <Button
            type='primary'
            icon={<IconEdit />}
            onClick={() => editModel(record)}
          ></Button>
          <Button
            icon={<IconDelete />}
            type='danger'
            onClick={() => deleteModel(record.name)}
          />
        </Space>
      ),
    },
  ];

  const updateModel = (name, field, value) => {
    if (isNaN(value)) {
      showError(t('Введите число'));
      return;
    }
    setModels((prev) =>
      prev.map((model) =>
        model.name === name ? { ...model, [field]: value } : model,
      ),
    );
  };

  const deleteModel = (name) => {
    setModels((prev) => prev.filter((model) => model.name !== name));
  };

  const calculateRatioFromTokenPrice = (tokenPrice) => {
    return tokenPrice / 2;
  };

  const calculateCompletionRatioFromPrices = (
    modelTokenPrice,
    completionTokenPrice,
  ) => {
    if (!modelTokenPrice || modelTokenPrice === '0') {
      showError(t('Цена модели не может быть 0'));
      return '';
    }
    return completionTokenPrice / modelTokenPrice;
  };

  const handleTokenPriceChange = (value) => {
    // Use a temporary variable to hold the new state
    let newState = {
      ...(currentModel || {}),
      tokenPrice: value,
      ratio: 0,
    };

    if (!isNaN(value) && value !== '') {
      const tokenPrice = parseFloat(value);
      const ratio = calculateRatioFromTokenPrice(tokenPrice);
      newState.ratio = ratio;
    }

    // Set the state with the complete updated object
    setCurrentModel(newState);
  };

  const handleCompletionTokenPriceChange = (value) => {
    // Use a temporary variable to hold the new state
    let newState = {
      ...(currentModel || {}),
      completionTokenPrice: value,
      completionRatio: 0,
    };

    if (!isNaN(value) && value !== '' && currentModel?.tokenPrice) {
      const completionTokenPrice = parseFloat(value);
      const modelTokenPrice = parseFloat(currentModel.tokenPrice);

      if (modelTokenPrice > 0) {
        const completionRatio = calculateCompletionRatioFromPrices(
          modelTokenPrice,
          completionTokenPrice,
        );
        newState.completionRatio = completionRatio;
      }
    }

    // Set the state with the complete updated object
    setCurrentModel(newState);
  };

  const addOrUpdateModel = (values) => {
    // Check if we're editing an existing model or adding a new one
    const existingModelIndex = models.findIndex(
      (model) => model.name === values.name,
    );

    if (existingModelIndex >= 0) {
      // Update existing model
      setModels((prev) =>
        prev.map((model, index) =>
          index === existingModelIndex
            ? {
                name: values.name,
                price: values.price || '',
                ratio: values.ratio || '',
                completionRatio: values.completionRatio || '',
              }
            : model,
        ),
      );
      setVisible(false);
      showSuccess(t('Модель обновлена'));
    } else {
      // Add new model
      // Check if model name already exists
      if (models.some((model) => model.name === values.name)) {
        showError(t('Имя модели уже существует'));
        return;
      }

      setModels((prev) => [
        {
          name: values.name,
          price: values.price || '',
          ratio: values.ratio || '',
          completionRatio: values.completionRatio || '',
        },
        ...prev,
      ]);
      setVisible(false);
      showSuccess(t('Модель добавлена'));
    }
  };

  const calculateTokenPriceFromRatio = (ratio) => {
    return ratio * 2;
  };

  const resetModalState = () => {
    setCurrentModel(null);
    setPricingMode('per-token');
    setPricingSubMode('ratio');
  };

  const editModel = (record) => {
    // Determine which pricing mode to use based on the model's current configuration
    let initialPricingMode = 'per-token';
    let initialPricingSubMode = 'ratio';

    if (record.price !== '') {
      initialPricingMode = 'per-request';
    } else {
      initialPricingMode = 'per-token';
      // We default to ratio mode, but could set to token-price if needed
    }

    // Set the pricing modes for the form
    setPricingMode(initialPricingMode);
    setPricingSubMode(initialPricingSubMode);

    // Create a copy of the model data to avoid modifying the original
    const modelCopy = { ...record };

    // If the model has ratio data and we want to populate token price fields
    if (record.ratio) {
      modelCopy.tokenPrice = calculateTokenPriceFromRatio(
        parseFloat(record.ratio),
      ).toString();

      if (record.completionRatio) {
        modelCopy.completionTokenPrice = (
          parseFloat(modelCopy.tokenPrice) * parseFloat(record.completionRatio)
        ).toString();
      }
    }

    // Set the current model
    setCurrentModel(modelCopy);

    // Open the modal
    setVisible(true);

    // Use setTimeout to ensure the form is rendered before setting values
    setTimeout(() => {
      if (formRef.current) {
        // Update the form fields based on pricing mode
        const formValues = {
          name: modelCopy.name,
        };

        if (initialPricingMode === 'per-request') {
          formValues.priceInput = modelCopy.price;
        } else if (initialPricingMode === 'per-token') {
          formValues.ratioInput = modelCopy.ratio;
          formValues.completionRatioInput = modelCopy.completionRatio;
          formValues.modelTokenPrice = modelCopy.tokenPrice;
          formValues.completionTokenPrice = modelCopy.completionTokenPrice;
        }

        formRef.current.setValues(formValues);
      }
    }, 0);
  };

  return (
    <>
      <Space vertical align='start' style={{ width: '100%' }}>
        <Space>
          <Button
            icon={<IconPlus />}
            onClick={() => {
              resetModalState();
              setVisible(true);
            }}
          >
            {t('Добавить модель')}
          </Button>
          <Button type='primary' icon={<IconSave />} onClick={SubmitData}>
            {t('Применить изменения')}
          </Button>
          <Input
            prefix={<IconSearch />}
            placeholder={t('Поиск по названию модели')}
            value={searchText}
            onChange={(value) => {
              setSearchText(value);
              setCurrentPage(1);
            }}
            style={{ width: 200 }}
          />
        </Space>
        <Table
          columns={columns}
          dataSource={pagedData}
          pagination={{
            currentPage: currentPage,
            pageSize: pageSize,
            total: filteredModels.length,
            onPageChange: (page) => setCurrentPage(page),
            formatPageText: (page) =>
              t('С {{start}} по {{end}} из {{total}}', {
                start: page.currentStart,
                end: page.currentEnd,
                total: filteredModels.length,
              }),
            showTotal: true,
            showSizeChanger: false,
          }}
        />
      </Space>

      <Modal
        title={
          currentModel &&
          currentModel.name &&
          models.some((model) => model.name === currentModel.name)
            ? t('Редактировать модель')
            : t('Добавить модель')
        }
        visible={visible}
        onCancel={() => {
          resetModalState();
          setVisible(false);
        }}
        onOk={() => {
          if (currentModel) {
            // If we're in token price mode, make sure ratio values are properly set
            const valuesToSave = { ...currentModel };

            if (
              pricingMode === 'per-token' &&
              pricingSubMode === 'token-price' &&
              currentModel.tokenPrice
            ) {
              // Calculate and set ratio from token price
              const tokenPrice = parseFloat(currentModel.tokenPrice);
              valuesToSave.ratio = (tokenPrice / 2).toString();

              // Calculate and set completion ratio if both token prices are available
              if (
                currentModel.completionTokenPrice &&
                currentModel.tokenPrice
              ) {
                const completionPrice = parseFloat(
                  currentModel.completionTokenPrice,
                );
                const modelPrice = parseFloat(currentModel.tokenPrice);
                if (modelPrice > 0) {
                  valuesToSave.completionRatio = (
                    completionPrice / modelPrice
                  ).toString();
                }
              }
            }

            // Clear price if we're in per-token mode
            if (pricingMode === 'per-token') {
              valuesToSave.price = '';
            } else {
              // Clear ratios if we're in per-request mode
              valuesToSave.ratio = '';
              valuesToSave.completionRatio = '';
            }

            addOrUpdateModel(valuesToSave);
          }
        }}
      >
        <Form getFormApi={(api) => (formRef.current = api)}>
          <Form.Input
            field='name'
            label={t('Название модели')}
            placeholder='strawberry'
            required
            disabled={
              currentModel &&
              currentModel.name &&
              models.some((model) => model.name === currentModel.name)
            }
            onChange={(value) =>
              setCurrentModel((prev) => ({ ...prev, name: value }))
            }
          />

          <Form.Section text={t('Режим ценообразования')}>
            <div style={{ marginBottom: '16px' }}>
              <RadioGroup
                type='button'
                value={pricingMode}
                onChange={(e) => {
                  const newMode = e.target.value;
                  const oldMode = pricingMode;
                  setPricingMode(newMode);

                  // Instead of resetting all values, convert between modes
                  if (currentModel) {
                    const updatedModel = { ...currentModel };

                    // Update formRef with converted values
                    if (formRef.current) {
                      const formValues = {
                        name: updatedModel.name,
                      };

                      if (newMode === 'per-request') {
                        formValues.priceInput = updatedModel.price || '';
                      } else if (newMode === 'per-token') {
                        formValues.ratioInput = updatedModel.ratio || '';
                        formValues.completionRatioInput =
                          updatedModel.completionRatio || '';
                        formValues.modelTokenPrice =
                          updatedModel.tokenPrice || '';
                        formValues.completionTokenPrice =
                          updatedModel.completionTokenPrice || '';
                      }

                      formRef.current.setValues(formValues);
                    }

                    // Update the model state
                    setCurrentModel(updatedModel);
                  }
                }}
              >
                <Radio value='per-token'>{t('Поминутная тарификация')}</Radio>
                <Radio value='per-request'>{t('Потарифная тарификация')}</Radio>
              </RadioGroup>
            </div>
          </Form.Section>

          {pricingMode === 'per-token' && (
            <>
              <Form.Section text={t('Способ задания цены')}>
                <div style={{ marginBottom: '16px' }}>
                  <RadioGroup
                    type='button'
                    value={pricingSubMode}
                    onChange={(e) => {
                      const newSubMode = e.target.value;
                      const oldSubMode = pricingSubMode;
                      setPricingSubMode(newSubMode);

                      // Handle conversion between submodes
                      if (currentModel) {
                        const updatedModel = { ...currentModel };

                        // Convert between ratio and token price
                        if (
                          oldSubMode === 'ratio' &&
                          newSubMode === 'token-price'
                        ) {
                          if (updatedModel.ratio) {
                            updatedModel.tokenPrice =
                              calculateTokenPriceFromRatio(
                                parseFloat(updatedModel.ratio),
                              ).toString();

                            if (updatedModel.completionRatio) {
                              updatedModel.completionTokenPrice = (
                                parseFloat(updatedModel.tokenPrice) *
                                parseFloat(updatedModel.completionRatio)
                              ).toString();
                            }
                          }
                        } else if (
                          oldSubMode === 'token-price' &&
                          newSubMode === 'ratio'
                        ) {
                          // Ratio values should already be calculated by the handlers
                        }

                        // Update the form values
                        if (formRef.current) {
                          const formValues = {};

                          if (newSubMode === 'ratio') {
                            formValues.ratioInput = updatedModel.ratio || '';
                            formValues.completionRatioInput =
                              updatedModel.completionRatio || '';
                          } else if (newSubMode === 'token-price') {
                            formValues.modelTokenPrice =
                              updatedModel.tokenPrice || '';
                            formValues.completionTokenPrice =
                              updatedModel.completionTokenPrice || '';
                          }

                          formRef.current.setValues(formValues);
                        }

                        setCurrentModel(updatedModel);
                      }
                    }}
                  >
                    <Radio value='ratio'>{t('По коэффициенту')}</Radio>
                    <Radio value='token-price'>{t('По цене')}</Radio>
                  </RadioGroup>
                </div>
              </Form.Section>

              {pricingSubMode === 'ratio' && (
                <>
                  <Form.Input
                    field='ratioInput'
                    label={t('Коэффициент модели')}
                    placeholder={t('Введите коэффициент модели')}
                    onChange={(value) =>
                      setCurrentModel((prev) => ({
                        ...(prev || {}),
                        ratio: value,
                      }))
                    }
                    initValue={currentModel?.ratio || ''}
                  />
                  <Form.Input
                    field='completionRatioInput'
                    label={t('Коэффициент автодополнения')}
                    placeholder={t('Введите коэффициент автодополнения')}
                    onChange={(value) =>
                      setCurrentModel((prev) => ({
                        ...(prev || {}),
                        completionRatio: value,
                      }))
                    }
                    initValue={currentModel?.completionRatio || ''}
                  />
                </>
              )}

              {pricingSubMode === 'token-price' && (
                <>
                  <Form.Input
                    field='modelTokenPrice'
                    label={t('Цена за токен')}
                    onChange={(value) => {
                      handleTokenPriceChange(value);
                    }}
                    initValue={currentModel?.tokenPrice || ''}
                    suffix={t('$/1M токенов')}
                  />
                  <Form.Input
                    field='completionTokenPrice'
                    label={t('Цена автодополнения')}
                    onChange={(value) => {
                      handleCompletionTokenPriceChange(value);
                    }}
                    initValue={currentModel?.completionTokenPrice || ''}
                    suffix={t('$/1M токенов')}
                  />
                </>
              )}
            </>
          )}

          {pricingMode === 'per-request' && (
            <Form.Input
              field='priceInput'
              label={t('Фиксированная цена (за раз)')}
              placeholder={t('Введите цену за раз')}
              onChange={(value) =>
                setCurrentModel((prev) => ({
                  ...(prev || {}),
                  price: value,
                }))
              }
              initValue={currentModel?.price || ''}
            />
          )}
        </Form>
      </Modal>
    </>
  );
}
