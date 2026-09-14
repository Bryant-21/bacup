; Original method fill for FO76's server-side SQRadio76QuestScript stub.
; Song selection itself is condition-driven in the scene; this only keeps the
; recently-played history that the picker reads.

Function UpdateRadio()
    LastSong04 = LastSong03
    LastSong03 = LastSong02
    LastSong02 = LastSong01
    LastSong01 = CurrentSong
EndFunction
