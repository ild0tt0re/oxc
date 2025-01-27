use std::rc::Rc;

use oxc_allocator::Vec;
use oxc_ast::{ast::*, AstBuilder};
use oxc_span::Span;
use oxc_syntax::operator::{AssignmentOperator, LogicalOperator};

use crate::{
    context::TransformerCtx,
    options::{TransformOptions, TransformTarget},
    utils::CreateVars,
};

/// ES2021: Logical Assignment Operators
///
/// References:
/// * <https://babel.dev/docs/babel-plugin-transform-logical-assignment-operators>
/// * <https://github.com/babel/babel/blob/main/packages/babel-plugin-transform-logical-assignment-operators>
pub struct LogicalAssignmentOperators<'a> {
    ast: Rc<AstBuilder<'a>>,
    ctx: TransformerCtx<'a>,

    vars: Vec<'a, VariableDeclarator<'a>>,
}

impl<'a> CreateVars<'a> for LogicalAssignmentOperators<'a> {
    fn ctx(&self) -> &TransformerCtx<'a> {
        &self.ctx
    }

    fn vars_mut(&mut self) -> &mut Vec<'a, VariableDeclarator<'a>> {
        &mut self.vars
    }
}

impl<'a> LogicalAssignmentOperators<'a> {
    pub fn new(
        ast: Rc<AstBuilder<'a>>,
        ctx: TransformerCtx<'a>,
        options: &TransformOptions,
    ) -> Option<Self> {
        (options.target < TransformTarget::ES2021 || options.logical_assignment_operators).then(
            || {
                let vars = ast.new_vec();
                Self { ast, ctx, vars }
            },
        )
    }

    pub fn transform_expression<'b>(&mut self, expr: &'b mut Expression<'a>) {
        let Expression::AssignmentExpression(assignment_expr) = expr else { return };

        // `&&=` `||=` `??=`
        let operator = match assignment_expr.operator {
            AssignmentOperator::LogicalAnd => LogicalOperator::And,
            AssignmentOperator::LogicalOr => LogicalOperator::Or,
            AssignmentOperator::LogicalNullish => LogicalOperator::Coalesce,
            _ => return,
        };

        // `a &&= c` -> `a && (a = c);`
        //               ^     ^ assign_target
        //               ^ left_expr

        let left_expr: Expression<'a>;
        let assign_target: SimpleAssignmentTarget<'a>;

        // TODO: refactor this block, add tests, cover private identifier
        match &assignment_expr.left {
            AssignmentTarget::SimpleAssignmentTarget(target) => match target {
                SimpleAssignmentTarget::AssignmentTargetIdentifier(ident) => {
                    left_expr = self.ast.identifier_reference_expression((*ident).clone());
                    assign_target = self.ast.simple_assignment_target_identifier((*ident).clone());
                }
                SimpleAssignmentTarget::MemberAssignmentTarget(member_expr) => {
                    let span = Span::default();
                    let op = AssignmentOperator::Assign;

                    // `a.b &&= c` -> `var _a; (_a = a).b && (_a.b = c)`
                    match &**member_expr {
                        MemberExpression::StaticMemberExpression(static_expr) => {
                            if let Some(ident) = self.maybe_generate_memoised(&static_expr.object) {
                                let right = self.ast.copy(&static_expr.object);
                                let mut expr = self.ast.copy(static_expr);
                                let target = AssignmentTarget::SimpleAssignmentTarget(
                                    self.ast.simple_assignment_target_identifier(ident.clone()),
                                );
                                expr.object =
                                    self.ast.assignment_expression(span, op, target, right);
                                left_expr = self.ast.member_expression(
                                    MemberExpression::StaticMemberExpression(expr),
                                );

                                let mut expr = self.ast.copy(static_expr);
                                expr.object = self.ast.identifier_reference_expression(ident);
                                assign_target =
                                    self.ast.simple_assignment_target_member_expression(
                                        MemberExpression::StaticMemberExpression(expr),
                                    );
                            } else {
                                left_expr = self.ast.member_expression(
                                    MemberExpression::StaticMemberExpression(
                                        self.ast.copy(static_expr),
                                    ),
                                );
                                assign_target = SimpleAssignmentTarget::MemberAssignmentTarget(
                                    self.ast.copy(member_expr),
                                );
                            };
                        }
                        // `a[b.y] &&= c;` ->
                        // `var _a, _b$y; (_a = a)[_b$y = b.y] && (_a[_b$y] = c);`
                        MemberExpression::ComputedMemberExpression(computed_expr) => {
                            if let Some(ident) = self.maybe_generate_memoised(&computed_expr.object)
                            {
                                let property_ident =
                                    self.maybe_generate_memoised(&computed_expr.expression);

                                let right = self.ast.copy(&computed_expr.object);
                                let mut expr = self.ast.copy(computed_expr);
                                let target = AssignmentTarget::SimpleAssignmentTarget(
                                    self.ast.simple_assignment_target_identifier(ident.clone()),
                                );
                                expr.object =
                                    self.ast.assignment_expression(span, op, target, right);
                                if let Some(property_ident) = &property_ident {
                                    let left = AssignmentTarget::SimpleAssignmentTarget(
                                        self.ast.simple_assignment_target_identifier(
                                            property_ident.clone(),
                                        ),
                                    );
                                    let right = self.ast.copy(&computed_expr.expression);
                                    expr.expression =
                                        self.ast.assignment_expression(span, op, left, right);
                                }
                                left_expr = self.ast.member_expression(
                                    MemberExpression::ComputedMemberExpression(expr),
                                );

                                // `(_a[_b$y] = c)` part
                                let mut expr = self.ast.copy(computed_expr);
                                expr.object = self.ast.identifier_reference_expression(ident);
                                if let Some(property_ident) = property_ident {
                                    expr.expression =
                                        self.ast.identifier_reference_expression(property_ident);
                                }
                                assign_target =
                                    self.ast.simple_assignment_target_member_expression(
                                        MemberExpression::ComputedMemberExpression(expr),
                                    );
                            } else {
                                let property_ident =
                                    self.maybe_generate_memoised(&computed_expr.expression);

                                // let right = self.ast.copy(&computed_expr.object);
                                let mut expr = self.ast.copy(computed_expr);
                                // let target = AssignmentTarget::SimpleAssignmentTarget(
                                // self.ast.simple_assignment_target_identifier(ident.clone()),
                                // );
                                // expr.object =
                                // self.ast.assignment_expression(span, op, target, right);
                                if let Some(property_ident) = &property_ident {
                                    let left = AssignmentTarget::SimpleAssignmentTarget(
                                        self.ast.simple_assignment_target_identifier(
                                            property_ident.clone(),
                                        ),
                                    );
                                    let right = self.ast.copy(&computed_expr.expression);
                                    expr.expression =
                                        self.ast.assignment_expression(span, op, left, right);
                                }
                                left_expr = self.ast.member_expression(
                                    MemberExpression::ComputedMemberExpression(expr),
                                );

                                let mut expr = self.ast.copy(computed_expr);
                                // expr.object = self.ast.identifier_reference_expression(ident);
                                if let Some(property_ident) = property_ident {
                                    expr.expression =
                                        self.ast.identifier_reference_expression(property_ident);
                                }
                                assign_target =
                                    self.ast.simple_assignment_target_member_expression(
                                        MemberExpression::ComputedMemberExpression(expr),
                                    );
                            };
                        }
                        MemberExpression::PrivateFieldExpression(_) => return,
                    }
                }
                // All other are TypeScript syntax.
                _ => return,
            },
            // It is a Syntax Error if AssignmentTargetType of LeftHandSideExpression is not simple.
            // So safe to return here.
            AssignmentTarget::AssignmentTargetPattern(_) => return,
        };

        let assign_op = AssignmentOperator::Assign;
        let assign_target = AssignmentTarget::SimpleAssignmentTarget(assign_target);
        let right = self.ast.move_expression(&mut assignment_expr.right);
        let right =
            self.ast.assignment_expression(Span::default(), assign_op, assign_target, right);

        let logical_expr = self.ast.logical_expression(Span::default(), left_expr, operator, right);

        *expr = logical_expr;
    }
}

// TODO: test all permutations
