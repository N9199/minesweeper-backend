use crate::solver::Solver;

use std::collections::{HashSet, VecDeque};
use std::fmt;
use std::time::Duration;

use itertools::iproduct;
use rand::seq::SliceRandom;
use rand::thread_rng;

use wasm_timer::Instant; //Should be behind a compile flag, else import time::Instant

#[derive(Clone, PartialEq, Debug, Copy)]
pub enum GameState {
    InProgress,
    Won,
    Lost,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BoardCellState {
    Discovered = 0,
    Blank = 1,
    Flagged = 2,
    Question = 3,
    Exploded = 4,
    Other,
}

#[derive(Clone, Eq)]
pub struct BoardCell {
    cell: u8,
}

impl PartialEq for BoardCell {
    fn eq(&self, other: &Self) -> bool {
        match (self.state(), other.state()) {
            (BoardCellState::Discovered, BoardCellState::Discovered) => {
                self.value() == other.value()
            }
            (BoardCellState::Blank, BoardCellState::Blank) => true,
            (BoardCellState::Blank, BoardCellState::Flagged) => true,
            (BoardCellState::Blank, BoardCellState::Question) => true,
            (BoardCellState::Flagged, BoardCellState::Blank) => true,
            (BoardCellState::Flagged, BoardCellState::Flagged) => true,
            (BoardCellState::Flagged, BoardCellState::Question) => true,
            (BoardCellState::Question, BoardCellState::Blank) => true,
            (BoardCellState::Question, BoardCellState::Flagged) => true,
            (BoardCellState::Question, BoardCellState::Question) => true,
            (_, _) => false,
        }
    }
}

impl fmt::Debug for BoardCell {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let out = format!("({}, {:?})", self.value(), self.state());
        write!(f, "{}", out)
    }
}

impl BoardCell {
    pub fn from_char(c: char) -> Self {
        if c.is_numeric() {
            Self::from_raw_parts(c.to_digit(10).unwrap() as u8, BoardCellState::Discovered)
        } else if c == 'm' {
            Self::from_raw_parts(15, BoardCellState::Blank)
        } else if c == '?' {
            Self::from_raw_parts(0, BoardCellState::Blank)
        } else {
            Self::from_raw_parts(0, BoardCellState::Other)
        }
    }
    fn from_raw_parts(value: u8, state: BoardCellState) -> Self {
        Self {
            cell: ((state as u8) << 4) + value,
        }
    }
    pub fn new() -> Self {
        Self { cell: 1 << 4 }
    }
    pub fn state(&self) -> BoardCellState {
        match self.cell >> 4 {
            0 => BoardCellState::Discovered,
            1 => BoardCellState::Blank,
            2 => BoardCellState::Flagged,
            3 => BoardCellState::Question,
            4 => BoardCellState::Exploded,
            _ => BoardCellState::Other,
        }
    }
    pub fn value(&self) -> u8 {
        self.cell & ((1 << 4) - 1)
    }
    pub fn click(&mut self) -> bool {
        if self.state() == BoardCellState::Blank {
            self.cell = self.value();
            if self.value() == 0 {
                return true;
            }
        }
        false
    }

    pub fn flag(&mut self) -> i8 {
        if self.state() != BoardCellState::Discovered {
            self.cell = self.value() + (((self.state() as u8) % 3 + 1) << 4);
        }
        match self.state() {
            BoardCellState::Question => -1,
            BoardCellState::Flagged => 1,
            _ => 0,
        }
    }
}

impl Default for BoardCell {
    fn default() -> Self {
        Self::new()
    }
}
pub type BoardCells = Vec<Vec<BoardCell>>;
#[derive(Debug)]
pub struct Board {
    board: BoardCells,
    pub rows: usize,
    pub cols: usize,
    pub mines: usize,
    pub game_state: GameState,
    start: bool,
    clicked_cells: usize,
    flagged_cells: i16,
    start_time: Option<Instant>, //Check Instant is behind compile flag for correctness.
    display_time: Duration,
    solver: Option<Solver>,
}

impl Board {
    pub fn new(rows: usize, cols: usize, mines: usize) -> Self {
        Board {
            board: (0..rows)
                .map(|_| (0..cols).map(|_| BoardCell::default()).collect())
                .collect(),
            rows,
            cols,
            mines,
            game_state: GameState::InProgress,
            start: false,
            clicked_cells: 0,
            flagged_cells: 0,
            start_time: None,
            display_time: Duration::ZERO,
            solver: None,
        }
    }

    pub fn start(&mut self, x: usize, y: usize, flag: bool) {
        //populate board
        log::debug!("Fill Board");
        let mut rng = thread_rng();
        let _place = x * self.cols + y;
        log::debug!("Create Mines");
        let mut places = iproduct!(-1..=1, -1..=1)
            .map(|(dx, dy)| (x as i32 + dx, y as i32 + dy))
            .filter(|(x, y)| 0 <= *x && *x < self.rows as i32 && 0 <= *y && *y < self.cols as i32)
            .map(|(x, y)| (x * self.cols as i32 + y) as usize)
            .collect::<Vec<usize>>();
        places.sort_unstable();
        let places = {
            let mut temp: Vec<(usize, usize)> = vec![(0, 0)];
            let (mut start, mut len, mut next) = (self.rows * self.cols, 0, self.rows * self.cols);
            for e in places {
                if e == next {
                    len += 1;
                    next += 1;
                } else {
                    if start != self.rows * self.cols {
                        temp.push((start, len));
                    }
                    start = e;
                    len = 1;
                    next = e + 1;
                }
            }
            temp.push((start, len));
            temp.push((self.cols * self.rows, 0));
            temp
        };
        //log::info!("{:?}", places);
        let mut pos = (0..(self.rows * self.cols - places.iter().fold(0, |acc, (_, x)| acc + x))) //Counting is hard
            .collect::<Vec<usize>>()
            .choose_multiple(&mut rng, self.mines)
            .copied()
            .collect::<Vec<usize>>();
        pos.sort_unstable();
        let mut delta = 0;
        let mut i = 0;
        let pos = pos
            .iter()
            .map(|a| {
                while places[i].0 <= (*a) + delta {
                    delta += places[i].1;
                    i += 1;
                }
                //log::info!("{} {} {}", a, delta, i);
                (*a) + delta * (flag as usize)
            })
            .map(|a| {
                //log::info!("a:{} x:{}", a, a/self.m);
                (a / self.cols, a % self.cols)
            })
            .collect::<Vec<(usize, usize)>>();
        //log::info!("self.m:{}", self.m);
        //log::info!("pos:{:?}", pos);
        log::debug!("Place Mines");
        for (x, y) in pos {
            self.board[x][y].cell = 15 + ((self.board[x][y].state() as u8) << 4);
            for (dx, dy) in iproduct!(-1..=1, -1..=1) {
                let x1 = x as i32 + dx;
                let y1 = y as i32 + dy;
                if 0 <= x1 && x1 < self.rows as i32 && 0 <= y1 && y1 < self.cols as i32 {
                    let x1 = x1 as usize;
                    let y1 = y1 as usize;
                    if self.board[x1][y1].value() != 15 {
                        self.board[x1][y1].cell += 1;
                        //log::info!("({},{}): {}", x1, y1, self.board[x1][y1].flags(),);
                    }
                }
            }
        }
        log::debug!("Finish Board Filling");

        self.solver = Solver::from_board(&self.board).into();
        self.solver.as_mut().unwrap().start();
        self.start = true;
        self.start_time = Some(Instant::now());
    }

    pub fn flag(&mut self, x: usize, y: usize) {
        if self.board[x][y].state() == BoardCellState::Discovered {
            self.click(x, y);
        }
        if !self.start {
            self.start(x, y, false);
        }
        self.flagged_cells += self.board[x][y].flag() as i16;
    }

    pub fn click(&mut self, x: usize, y: usize) {
        log::debug!("Clicked");
        if !self.start {
            self.start(x, y, true);
        }
        let mut q = VecDeque::new();
        let mut set = HashSet::new();
        log::debug!("Check if flagged");
        if self.board[x][y].state() == BoardCellState::Discovered {
            let mut count = 0;
            for (dx, dy) in iproduct!(-1..=1, -1..=1) {
                let x1 = x as i32 + dx;
                let y1 = y as i32 + dy;
                if 0 <= x1 && x1 < self.rows as i32 && 0 <= y1 && y1 < self.cols as i32 {
                    let x1 = x1 as usize;
                    let y1 = y1 as usize;
                    if self.board[x1][y1].state() == BoardCellState::Flagged {
                        count += 1;
                    }
                }
            }
            if count == self.board[x][y].value() {
                for (dx, dy) in iproduct!(-1..=1, -1..=1) {
                    let x1 = x as i32 + dx;
                    let y1 = y as i32 + dy;
                    if 0 <= x1 && x1 < self.rows as i32 && 0 <= y1 && y1 < self.cols as i32 {
                        let x1 = x1 as usize;
                        let y1 = y1 as usize;
                        if self.board[x1][y1].state() == BoardCellState::Blank {
                            q.push_back((x1, y1));
                            set.insert((x1, y1));
                        }
                    }
                }
            }
        }
        log::debug!("Check if clickable");
        if self.board[x][y].state() == BoardCellState::Blank {
            q.push_back((x, y));
            set.insert((x, y));
        }
        log::debug!("Check all discovered values");
        //Maybe optimize in future
        while let Some((x, y)) = q.pop_front() {
            //BFS
            if self.board[x][y].value() == 15 {
                self.board[x][y].click();
                self.game_state = GameState::Lost;
                self.board[x][y].cell = 15 + (4 << 4);
                return;
            }
            if self.board[x][y].state() == BoardCellState::Blank {
                self.clicked_cells += 1;
            }
            if self.board[x][y].click() {
                for (dx, dy) in iproduct!(-1..=1, -1..=1) {
                    let x1 = x as i32 + dx;
                    let y1 = y as i32 + dy;
                    if 0 <= x1 && x1 < self.rows as i32 && 0 <= y1 && y1 < self.cols as i32 {
                        let x1 = x1 as usize;
                        let y1 = y1 as usize;
                        if self.board[x1][y1].state() == BoardCellState::Blank
                            && !set.contains(&(x1, y1))
                        {
                            q.push_back((x1, y1));
                            set.insert((x1, y1));
                        }
                    }
                }
            }
        }
        log::debug!("Check if game is won");
        if self.clicked_cells + self.mines == self.cols * self.rows {
            self.game_state = GameState::Won;
        }
        log::debug!("Finish Click");
    }

    pub fn get_display_time(&self) -> Duration {
        match self.game_state {
            GameState::InProgress => match self.start_time {
                Some(start_time) => Instant::now() - start_time,
                None => Duration::ZERO,
            },
            _ => self.display_time,
        }
    }

    pub fn get_flagged_cells(&self) -> i16 {
        self.flagged_cells
    }

    pub fn get_board_cells(&self) -> &BoardCells {
        &self.board
    }

    pub fn update(&mut self) {
        self.display_time = self.get_display_time();
        if self.game_state != GameState::InProgress {
            for x in 0..self.rows {
                for y in 0..self.cols {
                    if self.board[x][y].value() != 15 {
                        self.board[x][y].click();
                    } else if self.game_state == GameState::Won {
                        self.board[x][y].cell = 15 + ((BoardCellState::Flagged as u8) << 4);
                    } else if self.board[x][y].state() != BoardCellState::Exploded {
                        self.board[x][y].cell = 15;
                    }
                }
            }
        }
    }
}

impl Default for Board {
    fn default() -> Self {
        Self::new(9, 9, 10)
    }
}
