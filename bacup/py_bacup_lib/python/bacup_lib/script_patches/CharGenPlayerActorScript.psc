; FO4's player base cannot carry this Actor script; the live chargen flow runs through
; CharGenPlayerAliasScript. What remains here only has to be valid FO4 source, so the
; FO76-only parts are dropped:
;   - OnEnterSelfieMenu / OnExitSelfieMenu / OnEnterLooksMenu / OnExitLooksMenu are FO76
;     engine events, and they only drove camera imods and server tutorial messages
;     (Message.ShowHelpMessageClientAndFlagTutorialSeen, SendRMIToServer,
;     GetSinglePlayerArray, the Player.ShowSelfieMenu static) that FO4 does not have.
;   - OnBeginState in the two later states only added/removed location-change event
;     filters (AddLocationChangeEventFilter); FO4's OnLocationChange is unfiltered.
;   - Player.IsNewCharacter() has no FO4 equivalent; being in the chargen location at
;     load is the entry condition.
; @drop-member OnEnterSelfieMenu
; @drop-member OnEnterLooksMenu
; @drop-member OnExitLooksMenu
; @drop-member OnExitSelfieMenu
; @drop-member OnBeginState

State waitingtoleavechargen
    Event OnLocationChange(Location akOldLoc, Location akNewLoc)
        If akOldLoc == CharGen06Location
            GoToState("HaveLeftChargen")
            Game.SetInChargen(False, False, False)
            ExitVaultMusic.Add()
        EndIf
    EndEvent
EndState

Auto State WaitingToStartChargen
    Event OnLoad()
        If Self == Game.GetPlayer() as ObjectReference && IsInLocation(CharGen06Location)
            HoldAtBlackImod.Apply(1.0)
            Game.SetCharGenHUDMode(1)
            GoToState("waitingtoleavechargen")
        EndIf
    EndEvent
EndState
