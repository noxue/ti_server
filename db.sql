CREATE TABLE IF NOT EXISTS `task` (
    `id` int(11) NOT NULL AUTO_INCREMENT,
    `worker` char(25) NOT NULL comment 'worker的ip端口信息',
    `product` varchar(100) NOT NULL comment '任务的产品名称',
    `status` tinyint NOT NULL comment '任务的状态',
    `create_time` timestamp NULL default CURRENT_TIMESTAMP comment '任务创建时间',
    `update_time` timestamp NULL default CURRENT_TIMESTAMP comment '任务更新时间',
} ENGINE = InnoDB DEFAULT CHARSET = utf8;

