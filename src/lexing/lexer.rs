use crate::stream;
use std::{ fmt, fmt::Debug, collections::HashMap, hash::Hash };

/// Describes a lexing state. Can include any number of transitions to other
/// states. When the lexer finds no appropriate transitions from this state,
/// the specified parsing function or target token of the `parse` member is
/// used to yield a token. A lexical error is produced if `parse` is of the
/// `Parse::Invalid` variant when a transition away from this state cannot be made).
pub struct State<'a, Key, Token> {
    pub parse: Parse<'a, Token>,
    pub transitions: Vec<Transition<'a, Key>>
}

/// When the lexer finds itself in a state that it cannot transition from, it
/// relies on the the value of State::parse in order to either yield a token or
/// produce a lexical error.
pub enum Parse<'a, Token> {
    To(Token), // For tokens that require no data from the lexeme (e.g. `BracketOpen`).
    ByFunction(&'a dyn Fn(&str) -> Token), // For tokens with information extracted from lexeme (e.g. `NumberLiteral`, `Identifier`).
    Invalid // For transitional states that do not produce a token (e.g. `PotentialReal`).
}

/// Describes a transition from one state to another (or itself). The lexer
/// decides whether this transition can be followed by calling that transition's
/// matching function with the character most recently read from the stream.
/// Should the matching funtion return turn true, the lexer will transition
/// states as specified.
pub struct Transition<'a, Key> {
    pub match_by: Match<'a>,
    pub to: Dest<Key>
}

/// Criteria by which it is decided whether the lexer should transition state
/// given the most recent character read from stream.
pub enum Match<'a> {
    ByChar(char), // Match by a single character.
    ByChars(Vec<char>), // Match by a number of possible characters.
    ByFunction(&'a dyn Fn(&char) -> bool), // Provide read charater to function which will return true if transition should be made.
    Any // Will always match.
}

/// Indicates how the lexer should transition state - either to remain on the
/// current state or to transition to a state with a given key.
pub enum Dest<Key> {
    ToSelf, // For remaining on the same state.
    To(Key) // For transitioning to other states.
}

/// Type allias for a hash map of state keys to states.
pub type States<'a, Key, Token> = HashMap<Key, State<'a, Key, Token>>;

/// A generic lexical analysis structure (not specific to lexing TILL - see
/// `lexing` module for how it is configured to do that).
/// 
/// * `Key` - Indicates the type to be used as a hash map key for referencing states.
/// * `Token` - Indicates the type of tokens yielded by the lexer.
pub struct Lexer<'a, Key, Token> {
    states: States<'a, Key, Token>,
    initial_state_key: Key,
    ignored: Vec<char>
}

impl<Key, Token> Lexer<'_, Key, Token>
where Key: Copy + Eq + Hash + Debug {
    /// Create a new lexer with it's own unique set of states.
    pub fn new(states: States<Key, Token>, initial_state_key: Key, ignored: Vec<char>) -> Lexer<'_, Key, Token> {
        Lexer {
            states,
            initial_state_key,
            ignored
        }
    }

    /// Consumes an input stream to produce an iterator that yields the tokens
    /// found through the analysis of said stream.
    pub fn input(&self, strm: stream::Stream) -> LexTokenIterator<'_, Key, Token> {
        LexTokenIterator {
            lxr: self,
            strm
        }
    }
}

/// Holds a token (indicates the type and contains any extra data), a raw lexeme
/// string, and the stream position from where the token was found.
#[derive(Debug, PartialEq)]
pub struct LexToken<Token>(pub Token, pub String, pub stream::Position);

#[derive(Debug, PartialEq)]
pub enum LexFailure {
    UnexpectedChar(char, String, stream::Position),
    UnexpectedEof(String, stream::Position)
}

impl fmt::Display for LexFailure {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LexFailure::UnexpectedChar(unexpected_char, lexeme, pos) => write!(f, "Encountered unexpected character {:?} while analysing lexeme {:?} at {}", unexpected_char, lexeme, pos),
            LexFailure::UnexpectedEof(lexeme, pos) => write!(f, "Encountered unexpected end of file while analysing lexeme {:?} at {}", lexeme, pos)
        }
    }
}

/// Iterator that yields `LexResult` instances containing a lexeme, stream potition,
/// and (assuming the lexeme is valid) a token. Created by the `Lexer::input` method.
pub struct LexTokenIterator<'a, Key, Token> {
    lxr: &'a Lexer<'a, Key, Token>,
    strm: stream::Stream
}

impl<Key, Token> Iterator for LexTokenIterator<'_, Key, Token>
where Key: Copy + Eq + Hash + Debug,
      Token: Clone + Debug {
    type Item = Result<LexToken<Token>, LexFailure>;

    /// Return the next token and lexeme in the current input stream.
    /// Returns `None` should the end of the current input stream have been
    /// reached.
    fn next(&mut self) -> Option<Self::Item> {
        log::info!("-- Next Token --");

        let mut current_key = self.lxr.initial_state_key;
        let mut lexeme = String::new();

        let mut unexpected_char: Option<char> = None;

        while let Some(chr) = self.strm.peek() {
            log::trace!("Peeking character: {:?}", chr);
            
            let state = get_state(&self.lxr.states, current_key);

            if let Some(new_key) = transition_state(current_key, &state.transitions, chr) {
                lexeme.push(chr);
                self.strm.advance();
                log::trace!("Character added to lexeme: {:?}", lexeme);

                current_key = new_key;
                log::trace!("State transitioned made - continuing...");
            }
            else {
                log::trace!("No appropriate transitions from state {:?} found!", current_key);

                if self.lxr.ignored.contains(&chr) && current_key == self.lxr.initial_state_key {
                    log::trace!("As currently in the initial state, character can be ignored - continuing...");
                    self.strm.advance(); // Advance the stream but don't add ignored character to lexeme.
                }
                else {
                    log::trace!("Character cannot be ignored - breaking...");
                    unexpected_char = Some(chr);
                    break;
                }
            }
        }

        if !lexeme.is_empty() {
            log::trace!("Attempting to parse lexeme...");
            Some(parse_lexeme(lexeme, unexpected_char, self.strm.get_pos().clone(), get_state(&self.lxr.states, current_key)))
        }
        else { None } // Nothing added to lexeme - assume stream had already reached end.
    }
}

/// Helper method to fetch and unwrap a `State` reference from a `States` hash map.
fn get_state<'a, Key, Token>(states: &'a States<Key, Token>, key: Key) -> &'a State<'a, Key, Token>
where Key: Eq + Hash + Debug {
    states.get(&key).expect(&format!("Lexer transitioned into an undefined state: {:?}", key))
}

/// Attempt to transition state given a vector of transitions and the current
/// input character. Will return `Some` holding the next state key should an
/// appropriate transition be found (whether to the current state or elsewhere).
/// `None` is returned when no appropriate transitions could be found.
fn transition_state<Key>(current_key: Key, transitions : &Vec<Transition<Key>>, chr: char) -> Option<Key>
where Key: Copy + Debug {
    for transition in transitions {
        let should_transition = match &transition.match_by {
            Match::ByChar(expected) => chr == *expected,
            Match::ByChars(possible) => possible.contains(&chr),
            Match::ByFunction(func) => func(&chr),
            Match::Any => true
        };

        if should_transition {
            return match &transition.to {
                Dest::To(new_key) => {
                    log::trace!("Transitioning state from {:?} to {:?}...", current_key, new_key);
                    Some(*new_key) // To new state...
                }

                Dest::ToSelf => {
                    log::trace!("Remaining in current state {:?}...", current_key);
                    Some(current_key) // To same state...
                }
            }
        }
    }

    None // No appropriate transition found (to self or otherwise) so return nothing.
}

/// Attempt to convert a lexeme into a token, assuming a given lexeme and final
/// lexer state (no more possible transitions could be made or reached end of
/// input stream).
fn parse_lexeme<Key, Token>(lexeme: String, next_chr: Option<char>, pos: stream::Position, final_state: &State<Key, Token>) -> Result<LexToken<Token>, LexFailure>
where Token: Clone + Debug {
    let potential_tok = match &final_state.parse {
        Parse::To(tok) => { Some(tok.clone()) }
        Parse::ByFunction(func) => { Some(func(&lexeme)) }
        Parse::Invalid => { None }
    };

    match potential_tok {
        Some(tok) => {
            log::debug!("At {} - lexeme {:?} parsed to token: {:?}", pos, lexeme, tok);
            Ok(LexToken(tok, lexeme, pos))
        }
        None => {
            log::debug!("At {} - could not parse to token from lexeme: {:?}", pos, lexeme);
            Err(match next_chr {
                Some(chr) => {
                    log::trace!("Failure to parse to token a result of unexpected character: {:?}", chr);
                    LexFailure::UnexpectedChar(chr, lexeme, pos)
                }
                None => {
                    log::trace!("Failure to parse to a token a result of reaching the stream end unexpectedly");
                    LexFailure::UnexpectedEof(lexeme, pos)
                }
            })
        }
    }
}