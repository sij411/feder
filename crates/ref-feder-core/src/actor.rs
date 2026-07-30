// Feder: A portable ActivityPub core for many runtimes.
// Copyright (C) 2026 Feder contributors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, version 3.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use feder_vocab::Actor;

pub trait ActorProvider {
    type Error;

    fn find_actor(&self, identifier: &str) -> Result<Option<Actor>, Self::Error>;
}

pub fn find_actor<R>(runtime: &R, identifier: &str) -> Result<Option<Actor>, R::Error>
where
    R: ActorProvider,
{
    runtime.find_actor(identifier)
}
