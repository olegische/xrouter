import React, { useEffect, useState } from 'react';
import {
  Table,
  Button,
  Input,
  Modal,
  Form,
  Space,
  Typography,
  Radio,
  Notification,
} from '@douyinfe/semi-ui';
import {
  IconDelete,
  IconPlus,
  IconSearch,
  IconSave,
  IconBolt,
} from '@douyinfe/semi-icons';
import { showError, showSuccess } from '../../../helpers';
import { API } from '../../../helpers';
import { useTranslation } from 'react-i18next';

export default function ModelRatioNotSetEditor(props) {
  const { t } = useTranslation();
  const [models, setModels] = useState([]);
  const [visible, setVisible] = useState(false);
  const [batchVisible, setBatchVisible] = useState(false);
  const [currentModel, setCurrentModel] = useState(null);
  const [searchText, setSearchText] = useState('');
  const [currentPage, setCurrentPage] = useState(1);
  const [pageSize, setPageSize] = useState(10);
  const [loading, setLoading] = useState(false);
  const [enabledModels, setEnabledModels] = useState([]);
  const [selectedRowKeys, setSelectedRowKeys] = useState([]);
  const [batchFillType, setBatchFillType] = useState('ratio');
  const [batchFillValue, setBatchFillValue] = useState('');
  const [batchRatioValue, setBatchRatioValue] = useState('');
  const [batchCompletionRatioValue, setBatchCompletionRatioValue] =
    useState('');
  const { Text } = Typography;
  // Define available page size options
  const pageSizeOptions = [10, 20, 50, 100];

  // Fetch all enabled models
  const getAllEnabledModels = async () => {
    try {
      const res = await API.get('/api/channel/models_enabled');
      const { success, message, data } = res.data;
      if (success) {
        setEnabledModels(data);
      } else {
        showError(message);
      }
    } catch (error) {
      console.error('Failed to fetch enabled models:', error);
      showError(t('Не удалось получить список активных моделей'));
    }
  };

  useEffect(() => {
    // Fetch all enabled models
    getAllEnabledModels();
  }, []);

  useEffect(() => {
    try {
      const modelPrice = JSON.parse(props.options.ModelPrice || '{}');
      const modelRatio = JSON.parse(props.options.ModelRatio || '{}');
      const completionRatio = JSON.parse(props.options.CompletionRatio || '{}');

      // Find all models that do not have price or ratio set
      const unsetModels = enabledModels.filter((modelName) => {
        const hasPrice = modelPrice[modelName] !== undefined;
        const hasRatio = modelRatio[modelName] !== undefined;

        // If the model does not have price or ratio set, show it
        return !hasPrice && !hasRatio;
      });

      // Create model data
      const modelData = unsetModels.map((name) => ({
        name,
        price: modelPrice[name] || '',
        ratio: modelRatio[name] || '',
        completionRatio: completionRatio[name] || '',
      }));

      setModels(modelData);
      // Clear selection
      setSelectedRowKeys([]);
    } catch (error) {
      console.error('JSON parse error:', error);
    }
  }, [props.options, enabledModels]);

  // Pagination utility function
  const getPagedData = (data, currentPage, pageSize) => {
    const start = (currentPage - 1) * pageSize;
    const end = start + pageSize;
    return data.slice(start, end);
  };

  // Handle page size change
  const handlePageSizeChange = (size) => {
    setPageSize(size);
    // Recalculate current page to avoid data loss
    const totalPages = Math.ceil(filteredModels.length / size);
    if (currentPage > totalPages) {
      setCurrentPage(totalPages || 1);
    }
  };

  // Before the return statement, process filtering and pagination logic
  const filteredModels = models.filter((model) =>
    searchText
      ? model.name.toLowerCase().includes(searchText.toLowerCase())
      : true,
  );

  // Then calculate paged data based on filtered models
  const pagedData = getPagedData(filteredModels, currentPage, pageSize);

  // Submit only changed models to the backend
  const SubmitData = async () => {
    setLoading(true);
    const output = {
      ModelPrice: JSON.parse(props.options.ModelPrice || '{}'),
      ModelRatio: JSON.parse(props.options.ModelRatio || '{}'),
      CompletionRatio: JSON.parse(props.options.CompletionRatio || '{}'),
    };

    try {
      // Data conversion - only process models that have been changed
      models.forEach((model) => {
        // Only update if the user has set a value
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
      // Re-fetch models that are not set
      getAllEnabledModels();
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
          placeholder={record.price !== '' ? t('Коэффициент модели') : t('Введите коэффициент модели')}
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
          placeholder={record.price !== '' ? t('Коэффициент автодополнения') : t('Введите коэффициент автодополнения')}
          disabled={record.price !== ''}
          onChange={(value) =>
            updateModel(record.name, 'completionRatio', value)
          }
        />
      ),
    },
  ];

  const updateModel = (name, field, value) => {
    if (value !== '' && isNaN(value)) {
      showError(t('Введите число'));
      return;
    }
    setModels((prev) =>
      prev.map((model) =>
        model.name === name ? { ...model, [field]: value } : model,
      ),
    );
  };

  // Add a new model, checking for duplicates
  const addModel = (values) => {
    // Check if model name exists, if so, reject addition
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
    showSuccess(t('Успешно добавлено'));
  };

  // Batch fill feature
  const handleBatchFill = () => {
    if (selectedRowKeys.length === 0) {
      showError(t('Сначала выберите модели для пакетной установки'));
      return;
    }

    if (batchFillType === 'bothRatio') {
      if (batchRatioValue === '' || batchCompletionRatioValue === '') {
        showError(t('Введите коэффициент модели и автодополнения'));
        return;
      }
      if (isNaN(batchRatioValue) || isNaN(batchCompletionRatioValue)) {
        showError(t('Введите корректное число'));
        return;
      }
    } else {
      if (batchFillValue === '') {
        showError(t('Введите значение'));
        return;
      }
      if (isNaN(batchFillValue)) {
        showError(t('Введите корректное число'));
        return;
      }
    }

    // Batch update models based on selected type
    setModels((prev) =>
      prev.map((model) => {
        if (selectedRowKeys.includes(model.name)) {
          if (batchFillType === 'price') {
            return {
              ...model,
              price: batchFillValue,
              ratio: '',
              completionRatio: '',
            };
          } else if (batchFillType === 'ratio') {
            return {
              ...model,
              price: '',
              ratio: batchFillValue,
            };
          } else if (batchFillType === 'completionRatio') {
            return {
              ...model,
              price: '',
              completionRatio: batchFillValue,
            };
          } else if (batchFillType === 'bothRatio') {
            return {
              ...model,
              price: '',
              ratio: batchRatioValue,
              completionRatio: batchCompletionRatioValue,
            };
          }
        }
        return model;
      }),
    );

    setBatchVisible(false);
    Notification.success({
      title: t('Пакетная установка выполнена успешно'),
      content: t('Для {{count}} моделей установлено значение {{type}}', {
        count: selectedRowKeys.length,
        type:
          batchFillType === 'price'
            ? t('Фиксированная цена')
            : batchFillType === 'ratio'
              ? t('Коэффициент модели')
              : batchFillType === 'completionRatio'
                ? t('Коэффициент автодополнения')
                : t('Коэффициент модели и автодополнения'),
      }),
      duration: 3,
    });
  };

  // Handle batch type change, clear values as needed
  const handleBatchTypeChange = (value) => {
    setBatchFillType(value);

    // Clear corresponding values when switching type
    if (value !== 'bothRatio') {
      setBatchFillValue('');
    } else {
      setBatchRatioValue('');
      setBatchCompletionRatioValue('');
    }
  };

  const rowSelection = {
    selectedRowKeys,
    onChange: (selectedKeys) => {
      setSelectedRowKeys(selectedKeys);
    },
  };

  return (
    <>
      <Space vertical align='start' style={{ width: '100%' }}>
        <Space>
          <Button icon={<IconPlus />} onClick={() => setVisible(true)}>
            {t('Добавить модель')}
          </Button>
          <Button
            icon={<IconBolt />}
            type='secondary'
            onClick={() => setBatchVisible(true)}
            disabled={selectedRowKeys.length === 0}
          >
            {t('Пакетная установка')} ({selectedRowKeys.length})
          </Button>
          <Button
            type='primary'
            icon={<IconSave />}
            onClick={SubmitData}
            loading={loading}
          >
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

        <Text>
          {t('Здесь отображаются только модели без установленной цены или коэффициента. После установки они будут автоматически удалены из списка.')}
        </Text>

        <Table
          columns={columns}
          dataSource={pagedData}
          rowSelection={rowSelection}
          rowKey='name'
          pagination={{
            currentPage: currentPage,
            pageSize: pageSize,
            total: filteredModels.length,
            onPageChange: (page) => setCurrentPage(page),
            onPageSizeChange: handlePageSizeChange,
            pageSizeOptions: pageSizeOptions,
            formatPageText: (page) =>
              t('С {{start}} по {{end}} из {{total}}', {
                start: page.currentStart,
                end: page.currentEnd,
                total: filteredModels.length,
              }),
            showTotal: true,
            showSizeChanger: true,
          }}
          empty={
            <div style={{ textAlign: 'center', padding: '20px' }}>
              {t('Нет моделей без настроек')}
            </div>
          }
        />
      </Space>

      {/* Модальное окно добавления модели */}
      <Modal
        title={t('Добавить модель')}
        visible={visible}
        onCancel={() => setVisible(false)}
        onOk={() => {
          currentModel && addModel(currentModel);
        }}
      >
        <Form>
          <Form.Input
            field='name'
            label={t('Название модели')}
            placeholder='strawberry'
            required
            onChange={(value) =>
              setCurrentModel((prev) => ({ ...prev, name: value }))
            }
          />
          <Form.Switch
            field='priceMode'
            label={
              <>
                {t('Режим ценообразования')}:
                {currentModel?.priceMode ? t('Фиксированная цена') : t('Режим коэффициента')}
              </>
            }
            onChange={(checked) => {
              setCurrentModel((prev) => ({
                ...prev,
                price: '',
                ratio: '',
                completionRatio: '',
                priceMode: checked,
              }));
            }}
          />
          {currentModel?.priceMode ? (
            <Form.Input
              field='price'
              label={t('Фиксированная цена (за раз)')}
              placeholder={t('Введите цену за раз')}
              onChange={(value) =>
                setCurrentModel((prev) => ({ ...prev, price: value }))
              }
            />
          ) : (
            <>
              <Form.Input
                field='ratio'
                label={t('Коэффициент модели')}
                placeholder={t('Введите коэффициент модели')}
                onChange={(value) =>
                  setCurrentModel((prev) => ({ ...prev, ratio: value }))
                }
              />
              <Form.Input
                field='completionRatio'
                label={t('Коэффициент автодополнения')}
                placeholder={t('Введите коэффициент автодополнения')}
                onChange={(value) =>
                  setCurrentModel((prev) => ({
                    ...prev,
                    completionRatio: value,
                  }))
                }
              />
            </>
          )}
        </Form>
      </Modal>

      {/* Модальное окно пакетной установки */}
      <Modal
        title={t('Пакетная установка параметров модели')}
        visible={batchVisible}
        onCancel={() => setBatchVisible(false)}
        onOk={handleBatchFill}
        width={500}
      >
        <Form>
          <Form.Section text={t('Тип настройки')}>
            <div style={{ marginBottom: '16px' }}>
              <Space>
                <Radio
                  checked={batchFillType === 'price'}
                  onChange={() => handleBatchTypeChange('price')}
                >
                  {t('Фиксированная цена')}
                </Radio>
                <Radio
                  checked={batchFillType === 'ratio'}
                  onChange={() => handleBatchTypeChange('ratio')}
                >
                  {t('Коэффициент модели')}
                </Radio>
                <Radio
                  checked={batchFillType === 'completionRatio'}
                  onChange={() => handleBatchTypeChange('completionRatio')}
                >
                  {t('Коэффициент автодополнения')}
                </Radio>
                <Radio
                  checked={batchFillType === 'bothRatio'}
                  onChange={() => handleBatchTypeChange('bothRatio')}
                >
                  {t('Коэффициент модели и автодополнения')}
                </Radio>
              </Space>
            </div>
          </Form.Section>

          {batchFillType === 'bothRatio' ? (
            <>
              <Form.Input
                field='batchRatioValue'
                label={t('Значение коэффициента модели')}
                placeholder={t('Введите коэффициент модели')}
                value={batchRatioValue}
                onChange={(value) => setBatchRatioValue(value)}
              />
              <Form.Input
                field='batchCompletionRatioValue'
                label={t('Значение коэффициента автодополнения')}
                placeholder={t('Введите коэффициент автодополнения')}
                value={batchCompletionRatioValue}
                onChange={(value) => setBatchCompletionRatioValue(value)}
              />
            </>
          ) : (
            <Form.Input
              field='batchFillValue'
              label={
                batchFillType === 'price'
                  ? t('Значение фиксированной цены')
                  : batchFillType === 'ratio'
                    ? t('Значение коэффициента модели')
                    : t('Значение коэффициента автодополнения')
              }
              placeholder={t('Введите значение')}
              value={batchFillValue}
              onChange={(value) => setBatchFillValue(value)}
            />
          )}

          <Text type='tertiary'>
            {t('Для выбранных ')} <Text strong>{selectedRowKeys.length}</Text>{' '}
            {t(' моделей будет установлено одинаковое значение')}
          </Text>
          <div style={{ marginTop: '8px' }}>
            <Text type='tertiary'>
              {t('Текущий тип настройки: ')}{' '}
              <Text strong>
                {batchFillType === 'price'
                  ? t('Фиксированная цена')
                  : batchFillType === 'ratio'
                    ? t('Коэффициент модели')
                    : batchFillType === 'completionRatio'
                      ? t('Коэффициент автодополнения')
                      : t('Коэффициент модели и автодополнения')}
              </Text>
            </Text>
          </div>
        </Form>
      </Modal>
    </>
  );
}
