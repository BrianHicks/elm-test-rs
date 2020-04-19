module ElmTestRunner.Reporter.Junit exposing (implementation)

{-| Json implementation of a reporter

@docs implementation

-}

import Array exposing (Array)
import Dict
import ElmTestRunner.Reporter.Interface exposing (Interface)
import ElmTestRunner.Result as TestResult exposing (TestResult)
import Xml
import Xml.Encode as Encode


{-| Provide a Junit XML implementation of a reporter, mostly for automated tools.
-}
implementation : Interface
implementation =
    { onBegin = always Nothing
    , onResult = always Nothing
    , onEnd = \_ -> Just << summary
    }


summary : Array TestResult -> String
summary results =
    let
        { totalDuration, nbFailed } =
            TestResult.summary results

        encodedTests =
            Array.toList results
                |> List.map encodeTestResult
                |> Encode.list

        suiteAttributes =
            Dict.fromList
                [ ( "name", Encode.string "elm-test-rs" )
                , ( "package", Encode.string "elm-test-rs" )
                , ( "tests", Encode.int (Array.length results) )

                -- "failures" should be used and not "failed"
                , ( "failures", Encode.int nbFailed )
                , ( "skipped", Encode.int 0 )
                , ( "time", Encode.float totalDuration )
                ]
    in
    Encode.encode 0 <|
        Encode.list
            [ Encode.string "<?xml version=\"1.0\"?>"
            , Encode.object [ ( "testsuite", suiteAttributes, encodedTests ) ]
            ]


encodeTestResult : TestResult -> Xml.Value
encodeTestResult result =
    let
        ( labels, duration, failures ) =
            case result of
                TestResult.Passed test ->
                    ( test.labels, test.duration, Encode.null )

                TestResult.Failed test ->
                    ( test.labels, test.duration, encodeFailures )

        ( class, name ) =
            classAndName labels

        attributesDict =
            Dict.fromList
                [ ( "classname", Encode.string class )
                , ( "name", Encode.string name )
                , ( "time", Encode.float duration )
                ]
    in
    Encode.object
        [ ( "testcase", attributesDict, failures ) ]


classAndName : List String -> ( String, String )
classAndName labels =
    case labels of
        [] ->
            ( "", "" )

        name :: classLabels ->
            ( String.join " " (List.reverse classLabels), name )


encodeFailures : Xml.Value
encodeFailures =
    Encode.object [ ( "failure", Dict.empty, Encode.null ) ]
