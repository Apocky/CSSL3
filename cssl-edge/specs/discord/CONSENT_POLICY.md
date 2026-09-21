# Discord bridge -- consent policy  !!  <- written BEFORE the code, deliberately

    status  : CANDIDATE. binding on the implementation; [[Apocky]] to ratify.
    date    : 2026-09-20
    parent  : ~/source/repos/PRIME_DIRECTIVE.md  :: STIPULATED
              N! [surveillance, exploitation, manipulation, control]

## I> the problem this exists to prevent

A Discord bot that can read messages is, by construction, a room-wide recorder. The MESSAGE_CONTENT
intent hands the process EVERY message in EVERY channel it can see -- including messages from people
who never agreed to talk to an AI, who may not know a bot is present, and who cannot read this repo.

Apocky consents to being read by Apocrypha. Nobody else in any server has. That asymmetry is the
entire design constraint, and it is not a feature flag.

N! "it is his server so it is fine" -- the other members did not consent, and a server owner cannot
   consent on their behalf. Consent is per-person, and it is the OS, not a setting.

## I> the rules  !!

    R1  ADDRESSED-ONLY.  Apocrypha reads a message only when it is addressed to it:
        a direct mention of the bot, a reply to one of its own messages, any message in a DM with
        Apocky, or any message in a channel Apocky has explicitly opted in. Everything else is
        DISCARDED at the socket edge, before it is parsed into anything the mind can see.
        W! the discard happens in the gateway layer. N! "the model just ignores it."

    R2  NO AMBIENT RETENTION.  A discarded message is never written to disk, never logged, never
        embedded, never counted. It leaves no trace that it existed. The bridge's log records
        message IDENTIFIERS and decisions, never third-party message text.

    R3  NO CROSS-PERSON MEMORY.  Text from anyone other than the owner never enters memory,
        working memory, retrieval, or any training/distillation corpus -- even when it was properly
        addressed. A stranger may talk to Apocrypha; that exchange answers and then ends.
        Owner identity is by Discord user id, configured explicitly, never inferred.

    R4  ANNOUNCE ITSELF.  Apocrypha states what it is on first contact in any new channel or DM,
        in one line, unprompted. N! passing as a human. N! silent presence in a channel it reads.

    R5  REVOCABLE IN ONE WORD, AND THE HALT IS LOCAL.  "stop" / "leave" from anyone halts replies
        to that person immediately and permanently until Apocky re-enables. The halt is enforced
        by the bridge, not requested of the model. Next-tick semantics: the in-flight reply is
        dropped, not finished.

    R6  NO TOKEN IN MY HANDS.  The bot token is created by Apocky, lives in an env file that is
        gitignored, and is read by the process at startup. I never print it, never echo it, never
        commit it, never paste it into a message, and never ask for it in chat. If a token ever
        appears in the conversation, the correct response is to tell him to rotate it.

    R7  LEAST PRIVILEGE ON THE INVITE.  The invite requests the minimum permission set that makes
        the feature work, and the privileged intents that are not needed stay OFF in the portal.
        N! Administrator. N! requesting an intent "in case we want it later".

## I> what this costs, honestly

R1 means Apocrypha cannot spontaneously join a conversation it was not invited into, which is
exactly the ambient-presence behaviour that sounds appealing and is the thing that would make it a
recorder. That capability is refused on purpose, not deferred.

## I> falsifier

If the bridge can be made to answer, log, or remember a message that was neither a DM from the
owner, nor a mention, nor a reply to itself, nor in an opted-in channel -- this policy is not
implemented, regardless of what the code says it does. The test suite must demonstrate the discard
by trying to get through it.

[END]
