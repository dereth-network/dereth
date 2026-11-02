use auberean_physics::geom::Matrix4;
use dereth_primitives::{Quat, Vec3};

use super::matrix_cases::CASES;
use super::{bits, check};

fn matrix(x: &[f32]) -> Matrix4 {
    let mut m = [0.0; 16];
    m.copy_from_slice(&x[..16]);
    Matrix4(m)
}

/// Every matrix operation answers as the September 2013 client's own code does, to the bit:
/// translations, turns about each axis, scales, the determinant, the adjoint, both inverses (the
/// full one dividing by the determinant of the adjoint it has just written, as the client does),
/// the transpose, the rotation a matrix holds and the matrix of a rotation, and the frame looking
/// from one point at another.
///
/// Behaviour: geometry.matrix.every-operation-answers-as-the-client-does
#[test]
fn every_matrix_operation_answers_as_the_client_does() {
    check(CASES, |label, x, _| {
        let mut m = Matrix4([9.0; 16]);
        let v = |i: usize| Vec3::new(x[i], x[i + 1], x[i + 2]);
        let out = match label {
            "translate" => {
                m.translate(x[0], x[1], x[2]);
                bits(&m.0)
            }
            "translate_vec" => {
                m.translate_vec(v(0));
                bits(&m.0)
            }
            "translate3" => {
                m.translate3(x[0], x[1]);
                bits(&m.0)
            }
            "scale" => {
                m.scale(x[0], x[1], x[2]);
                bits(&m.0)
            }
            "rotate_x" => {
                m.rotate_x(x[0]);
                bits(&m.0)
            }
            "rotate_y" => {
                m.rotate_y(x[0]);
                bits(&m.0)
            }
            "rotate_z" => {
                m.rotate_z(x[0]);
                bits(&m.0)
            }
            "determinant" => bits(&[matrix(x).determinant()]),
            "adjoint" => {
                m.adjoint(&matrix(x));
                bits(&m.0)
            }
            "inverse3" => {
                let r = m.inverse3(&matrix(x));
                let mut out = bits(&m.0);
                out.push(r.to_bits());
                out
            }
            "transpose" => {
                m.transpose(&matrix(x));
                bits(&m.0)
            }
            "inverse" => {
                let r = m.inverse(&matrix(x));
                let mut out = bits(&m.0);
                out.push(r.to_bits());
                out
            }
            "quaternion_from_rotation" => {
                let q = matrix(x).quaternion_from_rotation();
                bits(&[q.w, q.x, q.y, q.z])
            }
            "rotation_from_quaternion" => {
                m.rotation_from_quaternion(Quat::new(x[0], x[1], x[2], x[3]));
                bits(&m.0)
            }
            "look_at" => {
                let ok = m.look_at(v(0), v(3), v(6));
                let mut out = bits(&m.0);
                out.push(u32::from(ok));
                out
            }
            _ => return None,
        };
        Some(out)
    });
}
